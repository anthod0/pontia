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
    PontiaEvent, PontiaEventSource, PontiaEventType, get_workspace_record, upsert_workspace,
};

enum SessionManagementAction {
    Pin,
    Unpin,
    Archive,
    Unarchive,
}

impl SessionCommandService {
    /// Finds a Session created with a durable metadata token.
    ///
    /// This closes cross-service crash gaps without giving callers control over
    /// Session identity or introducing domain-specific Session creation paths.
    pub async fn find_session_by_creation_token(
        &self,
        metadata_key: &str,
        token: &str,
    ) -> Result<Option<String>> {
        let json_path = format!("$.{metadata_key}");
        let sessions = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM sessions WHERE json_extract(metadata, ?) = ? ORDER BY created_at LIMIT 2",
        )
        .bind(json_path)
        .bind(token)
        .fetch_all(&self.pool)
        .await?;
        match sessions.as_slice() {
            [] => Ok(None),
            [session_id] => Ok(Some(session_id.clone())),
            _ => Err(Error::StateConflict(format!(
                "multiple Sessions use creation token {token}"
            ))),
        }
    }

    pub async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> Result<CreateSessionOutcome> {
        self.clients.for_client(&request.client_type)?;

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
        let runtime_workspace_name = workspace_record
            .as_ref()
            .and_then(|workspace| workspace.name.clone());

        let session_id = new_session_id().to_string();
        let initial_slot = if request.initial_task.is_some() {
            Some(self.inbox.reserve_initial_input(&session_id).await)
        } else {
            None
        };
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
        let adapter = self.clients.for_client(&request.client_type)?;
        if !adapter.prepares_on_input() {
            ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.clone(),
                    None,
                    PontiaEventSource::ExternalApi,
                    request.client_type.clone(),
                    PontiaEventType::SessionStarting,
                    json!({}),
                ))
                .await?;
        }

        let runtime = adapter
            .start(
                &self.pontia_home,
                RuntimeStartRequest {
                    session_id: session_id.clone(),
                    client_type: request.client_type.clone(),
                    workspace: runtime_workspace.clone(),
                    workspace_name: runtime_workspace_name,
                    handle: request.handle.clone(),
                    role: request.role.clone(),
                    start_command: None,
                    environment: request.runtime_environment.clone(),
                },
            )
            .await?;
        let Some(runtime) = runtime else {
            drop(initial_slot);
            self.update_session_workspace(&session_id, workspace_record.as_ref())
                .await?;
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
        self.upsert_runtime_binding(&session_id, &runtime).await?;
        self.update_session_workspace(&session_id, workspace_record.as_ref())
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
            .ingest_in_process_ready_event(
                &request.client_type,
                &session_id,
                runtime.runtime_instance_id(),
            )
            .await?;

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
                runtime_instance_id: runtime.runtime_instance_id().map(str::to_string),
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
