use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;
use sqlx::SqlitePool;

/// Resolves once, then fences every execution against that same instance.
#[derive(Clone, Debug)]
pub struct ControlTarget {
    pub session_id: String,
    pub runtime_id: Option<String>,
    pub control_runtime_role: Option<&'static str>,
}

impl ControlTarget {
    pub async fn resolve(
        pool: &SqlitePool,
        clients: &crate::clients::ClientRegistry,
        session: &str,
        expected: Option<&str>,
    ) -> Result<Self> {
        let client: Option<String> =
            sqlx::query_scalar("SELECT client_type FROM sessions WHERE session_id = ?")
                .bind(session)
                .fetch_optional(pool)
                .await?;
        let role = client
            .as_deref()
            .and_then(|client| clients.control_runtime_role(client));
        let runtime_id = SqliteSessionRuntimeRepository::new(pool.clone())
            .runtime_id(session, role)
            .await?;
        if expected.is_some() && expected != runtime_id.as_deref() {
            return Err(Error::StateConflict(format!(
                "runtime is not the current runtime for session {session}"
            )));
        }
        Ok(Self {
            session_id: session.into(),
            runtime_id,
            control_runtime_role: role,
        })
    }

    pub async fn validate(&self, pool: &SqlitePool) -> Result<()> {
        // Keep the captured role: validation may run while the Session row is
        // write-locked by the caller, so it must query only runtime records.
        let current = SqliteSessionRuntimeRepository::new(pool.clone())
            .runtime_id(&self.session_id, self.control_runtime_role)
            .await?;
        if current != self.runtime_id {
            return Err(Error::StateConflict(
                "runtime binding changed before control execution".into(),
            ));
        }
        Ok(())
    }

    pub fn instance(&self) -> Result<&str> {
        self.runtime_id.as_deref().ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "session {} has no current runtime binding",
                self.session_id
            ))
        })
    }

    pub async fn tmux_pane(&self, pool: &SqlitePool) -> Result<(String, String)> {
        let binding = SqliteSessionRuntimeRepository::new(pool.clone())
            .tmux_pane_binding(&self.session_id)
            .await?
            .ok_or_else(|| Error::CapabilityUnavailable("missing tmux pane binding".into()))?;
        if binding.runtime_id != self.runtime_id {
            return Err(Error::StateConflict(
                "runtime binding changed before control execution".into(),
            ));
        }
        binding
            .socket_path
            .zip(binding.pane_id)
            .filter(|(socket, pane)| !socket.trim().is_empty() && !pane.trim().is_empty())
            .ok_or_else(|| Error::CapabilityUnavailable("missing tmux pane binding".into()))
    }
}
