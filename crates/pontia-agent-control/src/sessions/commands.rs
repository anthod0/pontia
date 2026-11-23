use pontia_core::{
    error::{Error, Result},
    ids::new_session_id,
};
use pontia_runtime::RuntimeStartRequest;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use serde_json::{Value, json};

use super::{
    CreateSessionOutcome, CreateSessionRequest, SessionCommandService, UpdateSessionRequest,
    validation::validate_handle,
};
use crate::{
    PontiaEvent, PontiaEventSource, PontiaEventType,
    workspaces::{require_active_workspace_id, resolve_session_directory},
};

enum SessionManagementAction {
    Pin,
    Unpin,
    Archive,
    Unarchive,
}

impl SessionCommandService {
    pub fn default_client_type(&self) -> Result<&'static str> {
        self.clients.registry().default_client_type()
    }
    pub async fn create_session(
        &self,
        mut request: CreateSessionRequest,
    ) -> Result<CreateSessionOutcome> {
        let _creation_guard = if request.ownership.is_some() {
            Some(self.creation_gate.lock().await)
        } else {
            None
        };
        if let Some(ownership) = &request.ownership {
            if ownership.application_id.is_empty() || ownership.reference.is_empty() {
                return Err(Error::Domain(
                    "Session ownership requires an application and reference".into(),
                ));
            }
            if let Some(session_id) = self.queries.find_session_by_ownership(ownership).await? {
                return Ok(CreateSessionOutcome {
                    data: json!({"session":self.queries.get_session(&session_id).await?}),
                    duplicate: true,
                });
            }
        }
        if request.client_type.is_empty() {
            request.client_type = self.clients.registry().default_client_type()?.to_string();
        }
        self.clients.for_client(&request.client_type)?;
        let client_key = match self
            .clients
            .registry()
            .get(&request.client_type)
            .and_then(|entry| entry.configuration.as_ref())
        {
            Some(configuration) => {
                let key = configuration.key(&request.metadata)?;
                configuration.validate_execution_policy(&key, &request.execution_policy)?;
                key
            }
            None if request.execution_policy != Default::default() => {
                return Err(Error::CapabilityUnavailable(
                    "Client cannot enforce the requested execution policy".into(),
                ));
            }
            None => "default".into(),
        };
        if let Some(model) = &request.model {
            if model.trim().is_empty() {
                return Err(Error::Domain("model must be non-empty".into()));
            }
            if !self
                .clients
                .registry()
                .get(&request.client_type)
                .and_then(|entry| entry.configuration.as_ref())
                .is_some_and(|config| config.supports_startup_model())
            {
                return Err(Error::CapabilityUnavailable(
                    "Model selection at startup is unavailable for this client".into(),
                ));
            }
        }
        let client_settings = crate::agent_clients::AgentClientService::new(
            self.pool.clone(),
            self.clients.registry().clone(),
        );
        let configuration = client_settings
            .for_creation_key(&request.client_type, &client_key)
            .await?;

        let instructions = crate::SessionInstructionsService::new(self.pool.clone())
            .with_clients(self.clients.registry().clone())
            .resolve(
                &request.client_type,
                request.system_instructions.as_deref(),
                request.include_standalone_session_instructions,
            )?;

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

        let (runtime_directory, workspace_record) =
            if let Some(workspace_id) = request.workspace_id.as_deref() {
                let workspace = require_active_workspace_id(&self.pool, workspace_id).await?;
                if !std::path::Path::new(&workspace.canonical_path).is_dir() {
                    return Err(Error::StateConflict(format!(
                        "Workspace directory {} is unavailable",
                        workspace.canonical_path
                    )));
                }
                (workspace.canonical_path.clone(), Some(workspace))
            } else if let Some(directory) = request.workspace.as_deref() {
                resolve_session_directory(&self.pool, &self.pontia_home, directory).await?
            } else {
                return Err(Error::Domain(
                    "A Session launch directory is required".into(),
                ));
            };
        if let Some(handle) = handle {
            let workspace = workspace_record.as_ref().ok_or_else(|| {
                Error::Domain("Session handles require a registered Workspace".into())
            })?;
            self.ensure_handle_available(&workspace.workspace_id, handle)
                .await?;
        }
        let runtime_workspace = Some(runtime_directory);
        let runtime_workspace_name = workspace_record
            .as_ref()
            .and_then(|record| record.name.clone());

        let session_id = new_session_id().to_string();
        let runtime_id = pontia_core::ids::new_runtime_id().to_string();
        let initial_slot = if request.initial_task.is_some() {
            Some(self.inbox.reserve_initial_input(&session_id).await)
        } else {
            None
        };
        let _launch = pontia_runtime::launch_lock::launch(&self.pontia_home)?;
        let ingest = self.event_ingest.clone();

        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.clone(),
                None,
                PontiaEventSource::ExternalApi,
                request.client_type.clone(),
                PontiaEventType::SessionCreated,
                json!({
                    "workspace": runtime_workspace,
                    "workspace_id": workspace_record.as_ref().map(|record| &record.workspace_id),
                    "title": request.title,
                    "handle": request.handle,
                    "role": request.role,
                    "description": request.description,
                    "session_instructions": instructions,
                    "metadata": request.metadata,
                    "ownership": request.ownership,
                    "runtime_environment": request.runtime_environment,
                    "execution_policy": request.execution_policy,
                    "launch_mode": request.launch_mode,
                }),
            ))
            .await?;
        let mut launch_guard = Some(self.control.lock_identity().await);
        let adapter = self.clients.for_client(&request.client_type)?;
        if !adapter.prepares_on_input() {
            ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.clone(),
                    None,
                    PontiaEventSource::ExternalApi,
                    request.client_type.clone(),
                    PontiaEventType::SessionStarting,
                    json!({"runtime_id": runtime_id}),
                ))
                .await?;
        }

        // Session clients complete their launch through an incoming attachment.
        if adapter.session_client().is_some() {
            drop(launch_guard.take());
        }
        let runtime = adapter
            .start(
                &self.pontia_home,
                RuntimeStartRequest {
                    launch_mode: request.launch_mode,
                    execution_policy: request.execution_policy,
                    model: request.model,
                    runtime_id: Some(runtime_id.clone()),
                    session_id: session_id.clone(),
                    client_type: request.client_type.clone(),
                    workspace: runtime_workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: request.handle.clone(),
                    role: request.role.clone(),
                    start_command: None,
                    environment: request.runtime_environment.clone(),
                },
                configuration,
            )
            .await;
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(error) => {
                ingest.ingest_pontia_event(PontiaEvent::new(&session_id, None, PontiaEventSource::RuntimeManager, &request.client_type, PontiaEventType::SessionError,
                    json!({"runtime_id": runtime_id, "reason": "startup_failed", "failure": {"message": error.to_string()}}))).await?;
                return Err(error);
            }
        };
        let Some(runtime) = runtime else {
            drop(initial_slot);
            if let Some(task) = request.initial_task {
                self.inbox
                    .submit_message(
                        &session_id,
                        crate::SubmitInboxMessageRequest {
                            input: task.input,
                            metadata: task.metadata,
                            delivery_policy: "after_idle".into(),
                            branch_target_turn_id: None,
                        },
                    )
                    .await?;
            }
            return Ok(CreateSessionOutcome {
                data: json!({"session":self.queries.get_session(&session_id).await?}),
                duplicate: false,
            });
        };
        self.upsert_runtime_binding(
            &session_id,
            &runtime,
            adapter.spec.adapter.control_runtime_role,
        )
        .await?;

        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.clone(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_in_process_ready_event(&request.client_type, &session_id, runtime.runtime_id())
            .await?;

        drop(launch_guard);
        let turns = self.turns.clone();
        let initial_turn = if let Some(task) = &request.initial_task {
            turns
                .prepare_initial(&session_id, &task.input, &task.metadata)
                .await?
        } else {
            None
        };
        let query = &self.queries;
        let session = query
            .get_session(&session_id)
            .await?
            .ok_or_else(|| Error::Domain("created session missing".into()))?;
        let initial_turn_id = initial_turn.as_ref().map(|turn| turn.turn_id.clone());
        let data = json!({ "session": session, "initial_turn": initial_turn });
        if let Some(task) = request.initial_task {
            let target = crate::runtime::ControlTarget {
                session_id: session_id.clone(),
                runtime_id: runtime.runtime_id().map(str::to_string),
                control_runtime_role: self
                    .clients
                    .registry()
                    .control_runtime_role(&request.client_type),
            };
            tokio::spawn(async move {
                let _initial_slot = initial_slot;
                if let Err(error) = turns
                    .dispatch_initial(
                        &target,
                        &task.input,
                        &task.metadata,
                        initial_turn_id.as_deref(),
                    )
                    .await
                {
                    tracing::warn!(%session_id, %error, "initial input dispatch failed");
                }
            });
        }

        Ok(CreateSessionOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn pin_session(&self, session_id: &str) -> Result<Value> {
        self.update_session_management_state(session_id, SessionManagementAction::Pin)
            .await
    }

    pub async fn unpin_session(&self, session_id: &str) -> Result<Value> {
        self.update_session_management_state(session_id, SessionManagementAction::Unpin)
            .await
    }

    pub async fn archive_session(&self, session_id: &str) -> Result<Value> {
        self.update_session_management_state(session_id, SessionManagementAction::Archive)
            .await
    }

    pub async fn unarchive_session(&self, session_id: &str) -> Result<Value> {
        self.update_session_management_state(session_id, SessionManagementAction::Unarchive)
            .await
    }

    async fn update_session_management_state(
        &self,
        session_id: &str,
        action: SessionManagementAction,
    ) -> Result<Value> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let rows_affected = match action {
            SessionManagementAction::Pin => repository.pin_session(session_id).await?,
            SessionManagementAction::Unpin => repository.unpin_session(session_id).await?,
            SessionManagementAction::Archive => repository.archive_session(session_id).await?,
            SessionManagementAction::Unarchive => repository.unarchive_session(session_id).await?,
        };
        if rows_affected == 0 {
            return Err(Error::NotFound(format!("session {session_id} not found")));
        }

        let query = &self.queries;
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("updated session missing".to_string()))?;
        Ok(json!({ "session": session }))
    }

    pub async fn update_session(
        &self,
        session_id: &str,
        request: UpdateSessionRequest,
    ) -> Result<Value> {
        let query = &self.queries;
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

        self.event_ingest
            .clone()
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                existing.client_type,
                PontiaEventType::SessionTitleUpdated,
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
