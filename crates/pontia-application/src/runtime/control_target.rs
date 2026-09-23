use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;
use sqlx::SqlitePool;

/// Resolves once, then fences every execution against that same instance.
#[derive(Clone, Debug)]
pub(crate) struct ControlTarget {
    pub session_id: String,
    pub runtime_instance_id: Option<String>,
}

impl ControlTarget {
    pub async fn resolve(pool: &SqlitePool, session: &str, expected: Option<&str>) -> Result<Self> {
        let runtime_instance_id = SqliteRuntimeBindingRepository::new(pool.clone())
            .runtime_instance_id(session)
            .await?;
        if expected.is_some() && expected != runtime_instance_id.as_deref() {
            return Err(Error::StateConflict(format!(
                "runtime is not the current runtime for session {session}"
            )));
        }
        Ok(Self {
            session_id: session.into(),
            runtime_instance_id,
        })
    }

    pub async fn validate(&self, pool: &SqlitePool) -> Result<()> {
        let current = Self::resolve(pool, &self.session_id, None).await?;
        if current.runtime_instance_id != self.runtime_instance_id {
            return Err(Error::StateConflict(
                "runtime binding changed before control execution".into(),
            ));
        }
        Ok(())
    }

    pub fn instance(&self) -> Result<&str> {
        self.runtime_instance_id.as_deref().ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "session {} has no current runtime binding",
                self.session_id
            ))
        })
    }

    pub async fn tmux_pane(&self, pool: &SqlitePool) -> Result<(String, String)> {
        let binding = SqliteRuntimeBindingRepository::new(pool.clone())
            .tmux_pane_binding(&self.session_id)
            .await?
            .ok_or_else(|| Error::CapabilityUnavailable("missing tmux pane binding".into()))?;
        if binding.runtime_instance_id != self.runtime_instance_id {
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
