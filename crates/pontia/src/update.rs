use std::path::PathBuf;

pub struct PreparedUpdate(pontia_update::PreparedUpdate);

pub async fn prepare() -> Result<PreparedUpdate, String> {
    pontia_update::prepare("pontia", &["pontia", "pontiad"])
        .await
        .map(PreparedUpdate)
}

impl PreparedUpdate {
    pub fn daemon_path(&self) -> PathBuf {
        self.0.directory().join("pontiad")
    }

    #[cfg(target_os = "linux")]
    pub fn ensure_no_unmanaged_daemon(&self) -> Result<(), String> {
        pontia_update::ensure_not_running(&self.daemon_path())
    }

    pub fn install(self, restart: impl FnMut() -> Result<(), String>) -> Result<(), String> {
        self.0.install(restart)
    }
}
