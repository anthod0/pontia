use super::validation::{client_dispatch_mode, client_readiness_mode, validate_handle};
use super::*;
use pontia_agent_clients::{DispatchMode, ReadinessMode, get_client_spec};

impl SessionCommandService {
    pub async fn create_session(
        &self,
        request: CreateSessionRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateSessionOutcome> {
        if !is_supported_client_type(&request.client_type) {
            return Err(pontia_core::error::Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response("create_session", key).await?
        {
            return Ok(CreateSessionOutcome {
                data: response,
                duplicate: true,
            });
        }

        let handle = request.handle.as_deref();
        if let Some(handle) = handle {
            validate_handle(handle)?;
        }
        if request.workspace.is_some() && request.workspace_id.is_some() {
            return Err(Error::Domain(
                "workspace and workspace_id cannot both be provided".to_string(),
            ));
        }
        if let Some(handle) = handle
            && request.workspace.is_none()
            && request.workspace_id.is_none()
        {
            return Err(Error::Domain(format!(
                "Cannot create session with handle {handle} because workspace is required."
            )));
        }

        let workspace_record = if let Some(workspace_id) = request.workspace_id.as_deref() {
            Some(
                get_workspace_record(&self.pool, workspace_id)
                    .await?
                    .ok_or_else(|| {
                        Error::NotFound(format!("workspace {workspace_id} not found"))
                    })?,
            )
        } else if let Some(workspace) = request.workspace.as_deref() {
            Some(upsert_workspace(&self.pool, workspace).await?)
        } else {
            None
        };
        if let (Some(workspace), Some(handle)) = (workspace_record.as_ref(), handle) {
            self.ensure_handle_available(&workspace.workspace_id, handle)
                .await?;
        }
        let runtime_workspace = workspace_record
            .as_ref()
            .map(|workspace| workspace.canonical_path.clone());

        let session_id = new_session_id().to_string();
        let ingest = EventIngestService::new(self.pool.clone());

        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::ExternalApi,
                request.client_type.clone(),
                EventType::SessionCreated,
                json!({
                    "workspace": runtime_workspace,
                    "title": request.title,
                    "handle": request.handle,
                    "role": request.role,
                    "description": request.description,
                    "execution_profile_id": request.execution_profile_id,
                    "execution_profile_version": request.execution_profile_version,
                    "metadata": request.metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::ExternalApi,
                request.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;

        let runtime = self.runtime.start_session(RuntimeStartRequest {
            session_id: session_id.clone(),
            client_type: request.client_type.clone(),
            workspace: runtime_workspace.clone(),
            handle: request.handle.clone(),
            role: request.role.clone(),
            agent_kind: pontia_agent_kind(&request.metadata),
            start_command: None,
        })?;
        self.upsert_runtime_binding(&session_id, &runtime).await?;
        self.update_session_workspace(&session_id, workspace_record.as_ref())
            .await?;

        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::RuntimeManager,
                request.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        if client_readiness_mode(&request.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.clone(),
                    None,
                    EventSource::RuntimeManager,
                    request.client_type.clone(),
                    EventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

        let initial_dispatch = if let Some(initial_task) = request.initial_task {
            let turn_id = new_turn_id().to_string();
            let client_spec = get_client_spec(&request.client_type).ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", request.client_type))
            })?;
            let plugin_owns_turn = client_spec.owns_initial_tmux_turn();
            if !plugin_owns_turn {
                ingest
                    .ingest_event(DomainEvent::new(
                        new_event_id().to_string(),
                        session_id.clone(),
                        Some(turn_id.clone()),
                        EventSource::ExternalApi,
                        request.client_type.clone(),
                        EventType::TurnCreated,
                        json!({
                            "input": { "summary": initial_task.input },
                            "metadata": initial_task.metadata,
                        }),
                    ))
                    .await?;
                ingest
                    .ingest_event(DomainEvent::new(
                        new_event_id().to_string(),
                        session_id.clone(),
                        Some(turn_id.clone()),
                        EventSource::ExternalApi,
                        request.client_type.clone(),
                        EventType::TurnQueued,
                        json!({}),
                    ))
                    .await?;
            }
            Some((
                turn_id.clone(),
                initial_task.input,
                client_dispatch_mode(&request.client_type)?,
                if plugin_owns_turn {
                    None
                } else {
                    Some(turn_id)
                },
            ))
        } else {
            None
        };
        let initial_turn_id = initial_dispatch
            .as_ref()
            .and_then(|(_, _, _, initial_turn_id)| initial_turn_id.clone());

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query.get_session(&session_id).await?.ok_or_else(|| {
            pontia_core::error::Error::Domain("created session missing".to_string())
        })?;
        let initial_turn = if let Some(turn_id) = initial_turn_id {
            query.get_turn(&session_id, &turn_id).await?
        } else {
            None
        };
        let data = json!({ "session": session, "initial_turn": initial_turn });

        if let Some(key) = idempotency_key {
            self.store_idempotency_response("create_session", key, &data)
                .await?;
        }

        if let Some((turn_id, input, dispatch_mode, _)) = initial_dispatch {
            let service = self.clone();
            let dispatch_session_id = session_id.clone();
            let dispatch_client_type = request.client_type.clone();
            let dispatch_runtime = runtime.clone();
            std::thread::spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        tracing::warn!(
                            session_id = %dispatch_session_id,
                            turn_id = %turn_id,
                            client_type = %dispatch_client_type,
                            error = %error,
                            "initial turn dispatch runtime creation failed"
                        );
                        return;
                    }
                };
                runtime.block_on(async move {
                    let result = match dispatch_mode {
                        DispatchMode::InProcessRecorded => {
                            service
                                .dispatch_initial_generic_turn(
                                    &dispatch_session_id,
                                    &turn_id,
                                    &dispatch_client_type,
                                    &input,
                                )
                                .await
                        }
                        DispatchMode::TmuxPaste => {
                            service
                                .wait_and_dispatch_initial_tui_turn(
                                    &dispatch_session_id,
                                    &turn_id,
                                    &dispatch_client_type,
                                    &input,
                                    &dispatch_runtime,
                                )
                                .await
                        }
                        DispatchMode::None => Ok(()),
                    };
                    if let Err(error) = result {
                        tracing::warn!(
                            session_id = %dispatch_session_id,
                            turn_id = %turn_id,
                            client_type = %dispatch_client_type,
                            error = %error,
                            "initial turn dispatch failed"
                        );
                    }
                });
            });
        }

        Ok(CreateSessionOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn update_session(
        &self,
        session_id: &str,
        request: UpdateSessionRequest,
    ) -> Result<Value> {
        let query = ExternalQueryService::new(self.pool.clone());
        let existing = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let title = request
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        EventIngestService::new(self.pool.clone())
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::ExternalApi,
                existing.client_type,
                EventType::SessionTitleUpdated,
                json!({ "title": title }),
            ))
            .await?;

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("updated session missing".to_string()))?;
        Ok(json!({ "session": session }))
    }
}
