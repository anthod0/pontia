use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    sync::Arc,
};

use pontia_agent_control::ClientControlStore;
use pontia_agent_control::{
    EventIngestService,
    client_contract::{
        AgentClientCapabilities, ClientExitMode, ClientExitOutcome, ClientLaunchRequest,
        ClientOperation, ClientSession, ClientSessionDetails, DeliveryError, DeliveryOperation,
        InputPayload, SessionCapabilityState,
    },
    control::InputReceipt,
    runtime::ControlTarget,
};
use pontia_core::{Error, Result};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
use serde_json::json;

pub(crate) struct PiSession {
    pub connections: crate::connections::PiConnections,
}
impl PiSession {
    fn validate_native_session(&self, key: &str, file: Option<&Path>) -> Result<()> {
        let path = file.ok_or_else(|| {
            Error::Unavailable(format!(
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
        let command = runtime
            .start_command
            .take()
            .unwrap_or_else(|| "pi --approve".into());
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

    async fn single_running_runtime(
        store: &ClientControlStore,
        session: &str,
    ) -> Result<Option<String>> {
        let mut running = store
            .queries()
            .session_runtimes(session)
            .await?
            .into_iter()
            .filter(|runtime| {
                runtime.role == crate::spec::CONTROL_RUNTIME_ROLE && runtime.state == "running"
            });
        Ok(match (running.next(), running.next()) {
            (Some(runtime), None) => Some(runtime.runtime_id),
            _ => None,
        })
    }
}

impl ClientSession for PiSession {
    fn native_history(
        &self,
        _store: ClientControlStore,
        binding: pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<Arc<dyn pontia_agent_control::client_contract::native_history::NativeHistory>> {
        Some(crate::history::PiHistory::new(
            binding,
            self.connections.clone(),
        ))
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

    fn start_turn<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
        _turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async move {
            let runtime = target.instance().map_err(DeliveryError::from)?;
            events
                .store()
                .runtime_readiness(Some(crate::spec::CONTROL_RUNTIME_ROLE))
                .wait_until_ready(&target.session_id, "pi", runtime)
                .await
                .map_err(DeliveryError::from)?;
            events
                .store()
                .validate_target(target)
                .await
                .map_err(DeliveryError::from)?;
            self.connections
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
            events
                .store()
                .validate_target(target)
                .await
                .map_err(DeliveryError::from)?;
            self.connections
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
            events
                .store()
                .validate_target(target)
                .await
                .map_err(DeliveryError::from)?;
            self.connections
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
            events
                .store()
                .runtime_readiness(Some(crate::spec::CONTROL_RUNTIME_ROLE))
                .wait_until_ready(&target.session_id, "pi", runtime)
                .await?;
            events.store().validate_target(target).await?;
            self.connections
                .replay(&target.session_id, runtime, replay_id)
                .await
                .map_err(DeliveryError::from)
        })
    }

    fn compact<'a>(
        &'a self,
        _events: EventIngestService,
        target: &'a ControlTarget,
        operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            self.connections
                .compact(&target.session_id, target.instance()?, operation_id)
                .await
        })
    }

    fn set_reasoning_effort<'a>(
        &'a self,
        _events: EventIngestService,
        target: &'a ControlTarget,
        reasoning_effort: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            self.connections
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
            events.store().validate_target(target).await?;
            let control = &self.connections;
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
                Err(error @ Error::Unavailable(_)) => {
                    let _guard = control.lock_identity().await;
                    events.store().validate_target(target).await?;
                    if control.connected_to(&target.session_id, runtime).await? {
                        return Err(error);
                    }
                    let fingerprint = events
                        .store()
                        .queries()
                        .session_runtime(runtime)
                        .await?
                        .filter(|runtime| runtime.state == "running")
                        .and_then(|runtime| runtime.process_fingerprint);
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
        store: ClientControlStore,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if let Some(binding) = store
                .bindings()
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
        store: ClientControlStore,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            Ok(match Self::single_running_runtime(&store, session).await? {
                Some(runtime) => self.connections.connected_to(session, &runtime).await?,
                None => false,
            })
        })
    }

    fn capability_state<'a>(
        &'a self,
        store: ClientControlStore,
        session: &'a str,
        implementation: &'a AgentClientCapabilities,
    ) -> ClientOperation<'a, SessionCapabilityState> {
        Box::pin(async move {
            let mut state = SessionCapabilityState::from(implementation.clone());
            let controls = match Self::single_running_runtime(&store, session).await? {
                Some(runtime) => {
                    self.connections
                        .reasoning_effort_support(session, &runtime)
                        .await?
                }
                None => None,
            };
            state.available.manual_compaction &= controls.is_some();
            state.available.reasoning_effort_control &= controls == Some(true);
            state.supported.reasoning_effort_control &= controls != Some(false);
            Ok(state)
        })
    }
    fn control_connected<'a>(
        &'a self,
        store: ClientControlStore,
        session: &'a str,
    ) -> ClientOperation<'a, Option<bool>> {
        Box::pin(async move {
            Ok(Some(
                match Self::single_running_runtime(&store, session).await? {
                    Some(runtime) => self.connections.connected_to(session, &runtime).await?,
                    None => false,
                },
            ))
        })
    }

    fn details<'a>(
        &'a self,
        _store: ClientControlStore,
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
        Error::Unavailable(format!(
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
