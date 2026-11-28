#![allow(dead_code)]

use std::{
    env,
    ffi::OsString,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use pontia_application::AppState;
use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::SqlitePool;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) struct TestApp {
    pub(crate) state: AppState,
    pub(crate) db: SqlitePool,
    pontia_home: tempfile::TempDir,
    workspace: tempfile::TempDir,
    _env: EnvGuard,
}

impl TestApp {
    pub(crate) fn builder() -> TestAppBuilder {
        TestAppBuilder::default()
    }

    pub(crate) async fn new() -> Self {
        Self::builder().build().await
    }

    pub(crate) fn pontia_home(&self) -> &tempfile::TempDir {
        &self.pontia_home
    }

    pub(crate) fn workspace(&self) -> &tempfile::TempDir {
        &self.workspace
    }

    pub(crate) fn temp_workspace(&self) -> tempfile::TempDir {
        tempfile::tempdir_in(self.pontia_home.path()).expect("workspace")
    }

    pub(crate) fn set_env(&mut self, key: &str, value: impl Into<OsString>) {
        self._env.set(key, value.into());
    }

    pub(crate) fn remove_env(&mut self, key: &str) {
        self._env.remove(key);
    }
}

#[derive(Default)]
pub(crate) struct TestAppBuilder {
    external_api_token: Option<Option<String>>,
    workspace_browser: Option<WorkspaceBrowserConfig>,
    file_picker: Option<FilePickerConfig>,
    in_memory_db: bool,
    database_name: Option<String>,
}

impl TestAppBuilder {
    pub(crate) fn external_api_token(mut self, token: Option<String>) -> Self {
        self.external_api_token = Some(token);
        self
    }

    pub(crate) fn workspace_browser(mut self, workspace_browser: WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = Some(workspace_browser);
        self
    }

    pub(crate) fn file_picker(mut self, file_picker: FilePickerConfig) -> Self {
        self.file_picker = Some(file_picker);
        self
    }

    pub(crate) fn in_memory_db(mut self) -> Self {
        self.in_memory_db = true;
        self
    }

    pub(crate) fn database_name(mut self, name: impl Into<String>) -> Self {
        self.database_name = Some(name.into());
        self
    }

    pub(crate) async fn build(self) -> TestApp {
        let pontia_home = tempfile::tempdir().expect("pontia home");
        let workspace = tempfile::tempdir_in(pontia_home.path()).expect("workspace");
        let env = EnvGuard::new();

        let db = self.open_database(Some(pontia_home.path())).await;
        pontia_agent_control::workspaces::upsert_canonical_workspace(
            &db,
            workspace.path().to_str().unwrap(),
            None,
        )
        .await
        .expect("register fixture Workspace");
        let state = self
            .build_app_state(db.clone(), pontia_home.path().to_path_buf())
            .await;

        TestApp {
            state,
            db,
            pontia_home,
            workspace,
            _env: env,
        }
    }

    pub(crate) async fn build_state(self) -> (AppState, tempfile::TempDir) {
        let pontia_home = tempfile::tempdir().expect("pontia home");
        let db = connect_sqlite("sqlite::memory:").await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        let state = self
            .build_app_state(db, pontia_home.path().to_path_buf())
            .await;
        (state, pontia_home)
    }

    async fn open_database(&self, pontia_home: Option<&std::path::Path>) -> SqlitePool {
        let database_url = if self.in_memory_db {
            "sqlite::memory:".to_string()
        } else {
            let db_name = self.database_name.as_deref().unwrap_or("test.db");
            let db_path = pontia_home
                .expect("filesystem database requires Pontia home")
                .join("data")
                .join(db_name);
            format!("sqlite://{}", db_path.display())
        };
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    async fn build_app_state(self, db: SqlitePool, pontia_home: PathBuf) -> AppState {
        let settings =
            pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
                db.clone(),
            );
        for client in ["generic", "pi", "codex"] {
            settings
                .save(client, "default", true)
                .await
                .expect("enable fixture client");
        }
        let mut builder = AppState::builder(
            pontia_agent_control::AgentControl::builder(db.clone(), pontia_home)
                .clients(crate::common::clients::clients())
                .build(),
        )
        .external_api_token(
            self.external_api_token
                .unwrap_or_else(|| Some("test-token".to_string())),
        );
        if let Some(workspace_browser) = self.workspace_browser {
            builder = builder.workspace_browser(workspace_browser);
        }
        if let Some(file_picker) = self.file_picker {
            builder = builder.file_picker(file_picker);
        }
        builder.build()
    }
}

pub(crate) struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    pub(crate) fn new() -> Self {
        Self {
            // Drop restores the environment before releasing the lock, including on panic.
            _lock: env_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            saved: Vec::new(),
        }
    }

    pub(crate) fn set(&mut self, key: &str, value: OsString) {
        self.save_once(key);
        unsafe {
            env::set_var(key, value);
        }
    }

    pub(crate) fn remove(&mut self, key: &str) {
        self.save_once(key);
        unsafe {
            env::remove_var(key);
        }
    }

    fn save_once(&mut self, key: &str) {
        if self.saved.iter().any(|(saved_key, _)| saved_key == key) {
            return;
        }
        self.saved.push((key.to_string(), env::var_os(key)));
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            unsafe {
                if let Some(value) = value {
                    env::set_var(key, value);
                } else {
                    env::remove_var(key);
                }
            }
        }
    }
}

pub(crate) async fn registered_workspace(state: &AppState) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("workspace");
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &state.db(),
        workspace.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .expect("register fixture Workspace");
    workspace
}

pub(crate) async fn workspace_id(state: &AppState) -> String {
    sqlx::query_scalar("SELECT workspace_id FROM workspaces WHERE state='active' LIMIT 1")
        .fetch_one(&state.db())
        .await
        .unwrap()
}
