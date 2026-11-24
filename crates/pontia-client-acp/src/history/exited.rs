use std::{collections::BTreeMap, path::Path, sync::Mutex};

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
    let environment: String = sqlx::query_scalar(
        "SELECT runtime_environment FROM sessions WHERE session_id=? AND state='exited'",
    )
    .bind(&source.session_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(unavailable)?;
    let binding = AgentBindingService::new(db.clone())
        .binding_for_session(&source.session_id)
        .await?
        .filter(|binding| {
            binding.id == source.id && binding.client_session_key == source.client_session_key
        })
        .ok_or_else(unavailable)?;
    let [agent, native] = crate::identity::decode(&binding.client_session_key)?;
    let definition = adapter.config.definition(&agent)?;
    let cwd = Path::new(&binding.launch_cwd);
    let policy = ControlQueryService::new(db.clone())
        .session_execution_policy(&source.session_id)
        .await?;
    let mut meta = SessionPolicy::resolve(definition.restricted_execution, &policy)?.meta;
    let mut launch = crate::peer::resolve_command(&definition.launch, cwd)?;
    if policy != Default::default()
        && matches!(
            definition.restricted_execution,
            Some(crate::RestrictedExecution::Antigravity)
        )
    {
        crate::antigravity::configure_launch(
            adapter.root.get().ok_or_else(unavailable)?,
            &policy,
            &mut launch,
        );
    }
    launch
        .env
        .extend(serde_json::from_str::<BTreeMap<String, String>>(
            &environment,
        )?);
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
    let mut replay = read(&launch, cwd, &bounds, load).await.map_err(|error| {
        Error::CapabilityUnavailable(format!(
            "source_unavailable: ACP history load failed: {error}"
        ))
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
    crate::stdio::run(launch, cwd, "ACP history load timed out", |streams| async {
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
                                *result = Err(unavailable())
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
                let result = collecting
                    .lock()
                    .unwrap()
                    .take()
                    .expect("collecting session/load");
                cx.send_request(CloseSessionRequest::new(native.clone()))
                    .block_task()
                    .await?;
                Ok(result)
            })
            .await
    })
    .await
}
