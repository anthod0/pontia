use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use pontia_agent_control::{
    AgentBindingService, ClientControlService, EventIngestService, RuntimeReadinessService,
    client_contract::{
        ClientExitMode, ClientExitOutcome, ClientLaunchRequest, ClientOperation, ClientSession,
        ClientSessionDetails, DeliveryError, DeliveryOperation, InputPayload,
    },
    control::InputReceipt,
    runtime::ControlTarget,
};
use pontia_core::{Error, Result};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
use serde_json::{Value, json};
use sqlx::SqlitePool;

pub(crate) struct PiSession {
    pub tui_command: Option<String>,
}
impl PiSession {
    fn launch_configured(
        &self,
        request: ClientLaunchRequest<'_>,
        configuration: &Value,
    ) -> Result<RuntimeStartResult> {
        let mut request = request;
        request.runtime.start_command = Some(format!(
            "{} --approve",
            crate::config::command(configuration)?
        ));
        self.launch(request)
    }

    fn validate_native_session(&self, key: &str, file: Option<&Path>) -> Result<()> {
        let path = file.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "cannot resume Pi session {key}: binding has no native session file"
            ))
        })?;
        verify_session_file(path, key)
    }

    fn launch(&self, request: ClientLaunchRequest<'_>) -> Result<RuntimeStartResult> {
        let mut runtime = request.runtime;
        if !runtime.environment.contains_key("PATH") {
            let path = std::env::join_paths(pontia_runtime::executable::inherited_search_path(
                Some(request.root),
            ))
            .map_err(|error| Error::Domain(error.to_string()))?;
            runtime.environment.insert(
                "PATH".into(),
                path.into_string()
                    .map_err(|_| Error::Domain("PATH is not UTF-8".into()))?,
            );
        }
        let command = runtime.start_command.take().unwrap_or_else(|| {
            let command = std::env::var("PONTIA_PI_TUI_COMMAND")
                .ok()
                .or_else(|| self.tui_command.clone())
                .unwrap_or_else(|| "pi".into());
            format!("{command} --approve")
        });
        let command = match &runtime.model {
            Some(model) => format!("{command} --model {}", quote(model)),
            None => command,
        };
        // Pi runs tools without approval or sandboxing. --approve trusts project resources.
        let tool_flags = match runtime.mode {
            pontia_core::SessionMode::Restricted => " --tools 'read,bash' --no-mcp --no-skills",
            pontia_core::SessionMode::Default | pontia_core::SessionMode::Yolo => "",
        };
        let mut session_args = match request.native_session_key {
            Some(key) => {
                self.validate_native_session(key, request.native_session_file)?;
                let path = request
                    .native_session_file
                    .expect("validated native session file");
                format!("--session {}", quote(&path.to_string_lossy()))
            }
            None => format!("--session-id {}", quote(&runtime.session_id)),
        };
        if let Some(effort) = &runtime.reasoning_effort {
            session_args.push_str(&format!(" --thinking {}", quote(effort)));
        }
        let background = runtime.launch_mode == pontia_core::LaunchMode::Background;
        let mode = if background { " --mode rpc" } else { "" };
        runtime.start_command = Some(format!("{command}{mode}{tool_flags} {session_args}"));
        let options = crate::SPEC.launch_options();
        let manager = pontia_runtime::GenericRuntimeManager;
        let mut result = if background {
            manager.start_detached_process(request.root, runtime, options.capabilities)?
        } else {
            manager.start_tmux(
                request.root,
                runtime,
                request.restart_count,
                request.reuse_pane,
                &options,
            )?
        };
        // Persist the reusable command, not the selector for this launch's native session.
        result.metadata["start_command"] = command.into();
        Ok(result)
    }

    fn control(events: &EventIngestService) -> Result<ClientControlService> {
        events
            .history_control()
            .ok_or_else(|| Error::CapabilityUnavailable("Pi control service is unavailable".into()))
    }

    async fn single_running_runtime(pool: &SqlitePool, session: &str) -> Result<Option<String>> {
        let runtimes: Vec<String> = sqlx::query_scalar(
            "SELECT runtime_id FROM session_runtimes WHERE session_id=? AND role=? AND state='running'",
        )
        .bind(session)
        .bind(crate::spec::CONTROL_RUNTIME_ROLE)
        .fetch_all(pool)
        .await?;
        Ok(match runtimes.as_slice() {
            [runtime] => Some(runtime.clone()),
            _ => None,
        })
    }
}

impl ClientSession for PiSession {
    fn provisions_runtime_binding(&self) -> bool {
        true
    }

    fn provision<'a>(
        &'a self,
        _events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move {
            self.launch(ClientLaunchRequest {
                root,
                runtime: request,
                restart_count: 0,
                reuse_pane: None,
                native_session_key: None,
                native_session_file: None,
            })
            .map(Some)
        })
    }

    fn provision_configured<'a>(
        &'a self,
        _events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
        configuration: Value,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move {
            self.launch_configured(
                ClientLaunchRequest {
                    root,
                    runtime: request,
                    restart_count: 0,
                    reuse_pane: None,
                    native_session_key: None,
                    native_session_file: None,
                },
                &configuration,
            )
            .map(Some)
        })
    }

    fn start_turn<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
        _turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async move {
            let runtime = target.instance().map_err(DeliveryError::from)?;
            RuntimeReadinessService::new(events.db(), Some(crate::spec::CONTROL_RUNTIME_ROLE))
                .wait_until_ready(&target.session_id, "pi", runtime)
                .await
                .map_err(DeliveryError::from)?;
            target
                .validate(&events.db())
                .await
                .map_err(DeliveryError::from)?;
            Self::control(&events)
                .map_err(DeliveryError::from)?
                .start_turn(&target.session_id, runtime, payload)
                .await?;
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }

    fn steer<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async move {
            target
                .validate(&events.db())
                .await
                .map_err(DeliveryError::from)?;
            Self::control(&events)
                .map_err(DeliveryError::from)?
                .steer(
                    &target.session_id,
                    target.instance().map_err(DeliveryError::from)?,
                    payload,
                )
                .await?;
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }

    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> DeliveryOperation<'a> {
        Box::pin(async move {
            target
                .validate(&events.db())
                .await
                .map_err(DeliveryError::from)?;
            Self::control(&events)
                .map_err(DeliveryError::from)?
                .interrupt(
                    &target.session_id,
                    target.instance().map_err(DeliveryError::from)?,
                )
                .await
        })
    }

    fn replay<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        replay_id: &'a str,
    ) -> DeliveryOperation<'a> {
        Box::pin(async move {
            let runtime = target.instance()?;
            RuntimeReadinessService::new(events.db(), Some(crate::spec::CONTROL_RUNTIME_ROLE))
                .wait_until_ready(&target.session_id, "pi", runtime)
                .await?;
            target.validate(&events.db()).await?;
            Self::control(&events)?
                .replay(&target.session_id, runtime, replay_id)
                .await
                .map_err(DeliveryError::from)
        })
    }

    fn compact<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            Self::control(&events)?
                .compact(&target.session_id, target.instance()?, operation_id)
                .await
        })
    }

    fn set_reasoning_effort<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        reasoning_effort: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            Self::control(&events)?
                .set_reasoning_effort(&target.session_id, target.instance()?, reasoning_effort)
                .await
        })
    }

    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            target.validate(&events.db()).await?;
            let control = Self::control(&events)?;
            let runtime = target.instance()?;
            if mode == ClientExitMode::IfIdle {
                return if control
                    .shutdown_if_idle(&target.session_id, runtime)
                    .await?
                {
                    Ok(ClientExitOutcome::Requested)
                } else {
                    Ok(ClientExitOutcome::Busy)
                };
            }
            match control.shutdown(&target.session_id, runtime).await {
                Ok(()) => Ok(ClientExitOutcome::Requested),
                Err(error @ Error::CapabilityUnavailable(_)) => {
                    let _guard = control.lock_identity().await;
                    target.validate(&events.db()).await?;
                    if control.connected_to(&target.session_id, runtime).await {
                        return Err(error);
                    }
                    let fingerprint: Option<String> = sqlx::query_scalar(
                        "SELECT process_fingerprint FROM session_runtimes WHERE runtime_id=? AND state='running'",
                    )
                    .bind(runtime)
                    .fetch_optional(&events.db())
                    .await?
                    .flatten();
                    let Some(fingerprint) = fingerprint else {
                        return Err(error);
                    };
                    pontia_runtime::GenericRuntimeManager
                        .terminate_process(&serde_json::from_str(&fingerprint)?)?;
                    Ok(ClientExitOutcome::Requested)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn resume_with_runtime<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        request: ClientLaunchRequest<'a>,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move { self.launch(request).map(Some) })
    }

    fn validate_resume<'a>(
        &'a self,
        pool: SqlitePool,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if let Some(binding) = AgentBindingService::new(pool)
                .binding_for_session(&target.session_id)
                .await?
            {
                self.validate_native_session(
                    &binding.client_session_key,
                    binding.client_session_file.as_deref().map(Path::new),
                )?;
            }
            Ok(())
        })
    }

    fn available<'a>(
        &'a self,
        pool: SqlitePool,
        control: Option<ClientControlService>,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Some(control) = control else {
                return Ok(false);
            };
            Ok(match Self::single_running_runtime(&pool, session).await? {
                Some(runtime) => control.connected_to(session, &runtime).await,
                None => false,
            })
        })
    }

    fn manual_compaction_available<'a>(
        &'a self,
        pool: SqlitePool,
        control: Option<ClientControlService>,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        self.available(pool, control, session)
    }

    fn reasoning_effort_control_available<'a>(
        &'a self,
        pool: SqlitePool,
        control: Option<ClientControlService>,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Some(control) = control else {
                return Ok(false);
            };
            if Self::single_running_runtime(&pool, session)
                .await?
                .is_none()
            {
                return Ok(false);
            }
            match control.reasoning_effort_control_available(session).await {
                Ok(available) => Ok(available),
                Err(Error::StateConflict(_) | Error::CapabilityUnavailable(_)) => Ok(false),
                Err(error) => Err(error),
            }
        })
    }

    fn control_connected<'a>(
        &'a self,
        pool: SqlitePool,
        control: Option<ClientControlService>,
        session: &'a str,
    ) -> ClientOperation<'a, Option<bool>> {
        Box::pin(async move {
            let Some(control) = control else {
                return Ok(Some(false));
            };
            Ok(Some(
                match Self::single_running_runtime(&pool, session).await? {
                    Some(runtime) => control.connected_to(session, &runtime).await,
                    None => false,
                },
            ))
        })
    }

    fn details<'a>(
        &'a self,
        _pool: SqlitePool,
        _session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async { Ok(ClientSessionDetails { data: json!({}) }) })
    }
}

fn verify_session_file(path: &Path, key: &str) -> Result<()> {
    let read_header = || -> Result<serde_json::Value> {
        let mut line = String::new();
        BufReader::new(File::open(path)?).read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    };
    let header = read_header().map_err(|error| {
        Error::CapabilityUnavailable(format!(
            "cannot resume Pi session from {}: {error}",
            path.display()
        ))
    })?;
    if header["type"] != "session" || header["id"].as_str() != Some(key) {
        return Err(Error::StateConflict(format!(
            "Pi session file {} does not match native session {key}",
            path.display()
        )));
    }
    Ok(())
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
