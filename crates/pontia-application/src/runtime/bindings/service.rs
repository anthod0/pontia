use std::path::PathBuf;

use pontia_agent_clients as agent_clients;
use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use super::{
    RuntimeBindingUpsertRequest,
    request::{is_fork_start, non_empty, validate_required},
};
use crate::upsert_workspace;

static RUNTIME_BINDING_UPSERT_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Clone)]
pub struct RuntimeBindingUpsertService {
    pub(super) pool: SqlitePool,
    pub(super) pontia_home: PathBuf,
}

impl RuntimeBindingUpsertService {
    pub fn new(pool: SqlitePool, pontia_home: PathBuf) -> Self {
        Self { pool, pontia_home }
    }

    pub async fn upsert(&self, request: RuntimeBindingUpsertRequest) -> Result<Value> {
        let _upsert_guard = RUNTIME_BINDING_UPSERT_LOCK.lock().await;
        validate_required("client_type", &request.client_type)?;
        validate_required("client_session_key", &request.client_session_key)?;
        let client_spec =
            agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", request.client_type))
            })?;
        let runtime_kind = client_spec.runtime_binding_kind().ok_or_else(|| {
            Error::Domain(format!(
                "runtime binding upsert does not support client_type {}",
                request.client_type
            ))
        })?;
        let tmux = request
            .tmux
            .as_ref()
            .ok_or_else(|| Error::Domain("runtime binding upsert requires tmux".to_string()))?;
        if non_empty(tmux.socket_path.as_deref()).is_none()
            || non_empty(tmux.pane_id.as_deref()).is_none()
        {
            return Err(Error::Domain(
                "runtime binding upsert requires tmux.socket_path and tmux.pane_id".to_string(),
            ));
        }

        let launch_cwd = request
            .launch_cwd
            .as_deref()
            .or(request.client_cwd.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::Domain("launch_cwd or client_cwd is required".to_string()))?;
        let workspace = upsert_workspace(&self.pool, launch_cwd).await?;

        let existing_session_id = if let Some(session_id) = non_empty(request.session_id.as_deref())
        {
            self.ensure_requested_session(&session_id, &request).await?;
            Some(session_id)
        } else {
            match self
                .session_id_for_client_session(&request.client_type, &request.client_session_key)
                .await?
            {
                Some(session_id) => Some(session_id),
                None => self.unbound_session_id_for_client_session(&request).await?,
            }
        };
        let session_id = match existing_session_id {
            Some(session_id) => {
                self.ensure_existing_binding_agrees(&session_id, &request)
                    .await?;
                self.ensure_active_runtime_is_not_replaced(&session_id, &request)
                    .await?;
                self.record_resume_lifecycle_for_exited_session(&session_id, &request)
                    .await?;
                session_id
            }
            None => self.create_bound_session(&request, &workspace).await?,
        };

        if is_fork_start(&request) {
            self.upsert_fork_lineage(&session_id, &request).await?;
        }

        SqliteSessionRepository::new(self.pool.clone())
            .update_session_workspace(
                &session_id,
                Some(&workspace.canonical_path),
                Some(&workspace.workspace_id),
            )
            .await?;

        self.confirm_binding(&session_id, runtime_kind, &request, &workspace, client_spec)
            .await
    }
}
