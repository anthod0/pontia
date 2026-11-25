use std::{path::Path, sync::Mutex};

use agent_client_protocol::{
    Agent, UntypedMessage,
    schema::ProtocolVersion,
    v1::{ConnectionTo, schema::*},
};
use pontia_agent_control::{
    AgentBindingService, ControlQueryService,
    client_contract::raw_transcripts::AgentBindingResolveRequest,
};
use pontia_core::{Error, Result};

use super::{Replay, unavailable};
use crate::{
    adapter::AcpAdapter,
    capabilities::{AgentBounds, Negotiated},
    instructions,
    restrictions::SessionPolicy,
};

pub(super) async fn load(
    adapter: &AcpAdapter,
    source: &AgentBindingResolveRequest,
) -> Result<Replay> {
    let db = adapter.pool.get().ok_or_else(unavailable)?;
    let state = sqlx::query_scalar::<_, String>("SELECT state FROM sessions WHERE session_id=?")
        .bind(&source.session_id)
        .fetch_one(db)
        .await?;
    if state != "exited" {
        return Err(if state == "error" {
            Error::Conflict {
                code: "timeline_history_load_failed",
                message: "ACP session recovery failed".into(),
            }
        } else {
            super::pending("Waiting for ACP session connection")
        });
    }
    let binding = AgentBindingService::new(db.clone())
        .binding_for_session(&source.session_id)
        .await?
        .filter(|binding| {
            binding.id == source.id && binding.client_session_key == source.client_session_key
        })
        .ok_or_else(super::identity_error)?;
    let [agent, native] = crate::identity::decode(&binding.client_session_key)?;
    let definition = adapter.config.definition(&agent)?;
    let cwd = Path::new(&binding.launch_cwd);
    if !cwd.is_dir() {
        return Err(Error::Conflict {
            code: "timeline_working_directory_unavailable",
            message: format!("ACP launch directory is unavailable: {}", cwd.display()),
        });
    }
    let mode = ControlQueryService::new(db.clone())
        .session_mode(&source.session_id)
        .await?;
    let policy = SessionPolicy::resolve(definition.restricted_execution, mode)?;
    let mut meta = policy.meta;
    let mut launch = crate::peer::resolve_command(&definition.launch, cwd)?;
    if matches!(
        definition.restricted_execution,
        Some(crate::RestrictedExecution::Antigravity)
    ) {
        crate::antigravity::configure_launch(
            adapter.root.get().ok_or_else(unavailable)?,
            mode,
            &mut launch,
        )?;
    }
    launch.env.extend(
        ControlQueryService::new(db.clone())
            .runtime_environment(&source.session_id)
            .await?,
    );
    if let Some(profile) = policy.profile {
        profile.configure_launch(&mut launch);
    }
    let mut load = LoadSessionRequest::new(SessionId::new(native), cwd.to_owned());
    let mut prefix = None;
    instructions::restore_load_meta(&mut meta, &binding.metadata, db, &source.session_id).await?;
    if binding.metadata["system_prompt_injected"] == true
        && binding.metadata["new_session_instructions_meta_key"].is_null()
    {
        let text = instructions::bound_prompt(db, &source.session_id).await?;
        if binding.metadata["first_prompt_instructions"] == true {
            prefix = Some(crate::instructions::prompt_prefix(&text));
        } else {
            load.system_prompt = Some(SystemPrompt::new(vec![ContentBlock::Text(
                TextContent::new(text),
            )]));
        }
    }
    load.meta = (!meta.is_empty()).then_some(meta);
    let bounds = AgentBounds::of(&agent, Some(definition));
    let mut replay = read(&launch, cwd, &bounds, load)
        .await
        .map_err(|error| match error {
            Error::Conflict { .. } => error,
            error => Error::Conflict {
                code: "timeline_history_load_failed",
                message: format!("ACP session/load failed: {error}"),
            },
        })?;
    replay.finish(prefix.as_deref());
    Ok(replay)
}

async fn read(
    launch: &crate::AcpLaunch,
    cwd: &Path,
    bounds: &AgentBounds,
    load: LoadSessionRequest,
) -> Result<Replay> {
    let collecting: Mutex<Option<Result<Replay>>> = Mutex::new(None);
    let native = load.session_id.clone();
    let completed = Mutex::new(None);
    let operation = crate::stdio::run(launch, cwd, "ACP history load timed out", |streams| async {
        agent_client_protocol::Client
            .v1()
            .on_receive_notification(
                async |message: UntypedMessage, _cx| {
                    let mut slot = collecting.lock().unwrap();
                    let Some(result) = slot.as_mut() else {
                        return Ok(());
                    };
                    if let Ok(history) = result {
                        let params = message.params();
                        match message.method() {
                            "session/update" | "_x.ai/session/update"
                                if params["sessionId"].as_str() != Some(native.0.as_ref()) =>
                            {
                                *result = Err(super::identity_error())
                            }
                            "session/update" => {
                                if let Err(error) = history.update(bounds, &params["update"]) {
                                    *result = Err(error);
                                }
                            }
                            "_x.ai/session/update"
                                if bounds.compaction == Some(crate::CompactionProtocol::Grok) =>
                            {
                                history.checkpoint(params)
                            }
                            _ => {}
                        }
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(streams, async |cx: ConnectionTo<Agent>| {
                let init = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                if init.protocol_version != ProtocolVersion::V1 {
                    return Ok(Err(unavailable()));
                }
                if let Err(error) = Negotiated::from_initialize(&init, bounds) {
                    return Ok(Err(error));
                }
                *collecting.lock().unwrap() = Some(Ok(Replay::default()));
                cx.send_request(load).block_task().await?;
                *completed.lock().unwrap() = collecting.lock().unwrap().take();
                let close = cx
                    .send_request(CloseSessionRequest::new(native.clone()))
                    .block_task();
                let _ = tokio::time::timeout(std::time::Duration::from_secs(1), close).await;
                Ok(Ok(()))
            })
            .await
    })
    .await;
    completed
        .into_inner()
        .unwrap()
        .unwrap_or_else(|| operation.and_then(|()| Err(unavailable())))
}
