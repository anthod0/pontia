#![allow(dead_code)]

use std::{
    env,
    ffi::OsString,
    sync::{Mutex, MutexGuard, OnceLock},
};

use pontia_application::AppState;
use pontia_config::{FilePickerConfig, GraphRuntimeConfig, WorkspaceBrowserConfig};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::SqlitePool;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub struct TestApp {
    pub state: AppState,
    pub db: SqlitePool,
    pontia_home: tempfile::TempDir,
    workspace: tempfile::TempDir,
    _db_dir: Option<tempfile::TempDir>,
    _env: EnvGuard,
}

impl TestApp {
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder::default()
    }

    pub async fn new() -> Self {
        Self::builder().build().await
    }

    pub fn pontia_home(&self) -> &tempfile::TempDir {
        &self.pontia_home
    }

    pub fn workspace(&self) -> &tempfile::TempDir {
        &self.workspace
    }

    pub fn temp_workspace(&self) -> tempfile::TempDir {
        tempfile::tempdir().expect("workspace")
    }
}

#[derive(Default)]
pub struct TestAppBuilder {
    external_api_token: Option<Option<String>>,
    graph: Option<GraphRuntimeConfig>,
    workspace_browser: Option<WorkspaceBrowserConfig>,
    file_picker: Option<FilePickerConfig>,
    in_memory_db: bool,
    database_name: Option<String>,
    pi_runtime_stub: bool,
}

impl TestAppBuilder {
    pub fn external_api_token(mut self, token: Option<String>) -> Self {
        self.external_api_token = Some(token);
        self
    }

    pub fn graph(mut self, graph: GraphRuntimeConfig) -> Self {
        self.graph = Some(graph);
        self
    }

    pub fn workspace_browser(mut self, workspace_browser: WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = Some(workspace_browser);
        self
    }

    pub fn file_picker(mut self, file_picker: FilePickerConfig) -> Self {
        self.file_picker = Some(file_picker);
        self
    }

    pub fn in_memory_db(mut self) -> Self {
        self.in_memory_db = true;
        self
    }

    pub fn database_name(mut self, name: impl Into<String>) -> Self {
        self.database_name = Some(name.into());
        self
    }

    pub fn pi_runtime_stub(mut self, enabled: bool) -> Self {
        self.pi_runtime_stub = enabled;
        self
    }

    pub async fn build(self) -> TestApp {
        let pontia_home = tempfile::tempdir().expect("pontia home");
        let workspace = tempfile::tempdir().expect("workspace");
        let mut env = EnvGuard::new();
        env.set("PONTIA_HOME", pontia_home.path().as_os_str().to_owned());
        if self.pi_runtime_stub {
            env.set(
                "PONTIA_PI_TUI_COMMAND",
                OsString::from("cat >> \"$PONTIA_WORKSPACE/pi-tui-input.log\""),
            );
        }

        let (db, db_dir) = self.open_database().await;
        let state = self.build_app_state(db.clone());

        TestApp {
            state,
            db,
            pontia_home,
            workspace,
            _db_dir: db_dir,
            _env: env,
        }
    }

    pub async fn build_state(self) -> AppState {
        let (db, db_dir) = self.open_database().await;
        let state = self.build_app_state(db);
        if let Some(dir) = db_dir {
            let _kept_dir = dir.keep();
        }
        state
    }

    async fn open_database(&self) -> (SqlitePool, Option<tempfile::TempDir>) {
        let (db, db_dir) = if self.in_memory_db {
            let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
            (db, None)
        } else {
            let dir = tempfile::tempdir().expect("tempdir");
            let db_name = self.database_name.as_deref().unwrap_or("test.db");
            let db_path = dir.path().join(db_name);
            let database_url = format!("sqlite://{}", db_path.display());
            let db = connect_sqlite(&database_url).await.expect("connect");
            (db, Some(dir))
        };
        run_migrations(&db).await.expect("migrate");
        (db, db_dir)
    }

    fn build_app_state(self, db: SqlitePool) -> AppState {
        let mut builder = AppState::builder(db).external_api_token(
            self.external_api_token
                .unwrap_or_else(|| Some("test-token".to_string())),
        );
        if let Some(graph) = self.graph {
            builder = builder.graph(graph);
        }
        if let Some(workspace_browser) = self.workspace_browser {
            builder = builder.workspace_browser(workspace_browser);
        }
        if let Some(file_picker) = self.file_picker {
            builder = builder.file_picker(file_picker);
        }
        builder.build()
    }
}

pub struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    pub fn new() -> Self {
        Self {
            _lock: env_lock().lock().expect("test env lock"),
            saved: Vec::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: OsString) {
        self.save_once(key);
        unsafe {
            env::set_var(key, value);
        }
    }

    pub fn remove(&mut self, key: &str) {
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
