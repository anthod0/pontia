use super::AppStateBuilder;
use pontia_agent_control::AgentControl;
use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig};
use std::sync::Arc;
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
    external_api_token: Option<String>,
}
struct AppStateInner {
    control: AgentControl,
    workspace_browser: WorkspaceBrowserConfig,
    file_picker: FilePickerConfig,
    git_refresh: crate::GitRefreshCoordinator,
    idempotency: crate::IdempotencyCoordinator,
}
impl AppState {
    pub fn builder(control: AgentControl) -> AppStateBuilder {
        AppStateBuilder::new(control)
    }
    pub(super) fn from_builder(builder: AppStateBuilder) -> Self {
        Self {
            external_api_token: builder.external_api_token,
            inner: Arc::new(AppStateInner {
                control: builder.control,
                workspace_browser: builder.workspace_browser,
                file_picker: builder.file_picker,
                git_refresh: builder.git_refresh,
                idempotency: builder.idempotency,
            }),
        }
    }
    pub fn control(&self) -> &AgentControl {
        &self.inner.control
    }
    pub fn db(&self) -> sqlx::SqlitePool {
        self.control().db()
    }
    pub fn pontia_home(&self) -> &std::path::Path {
        self.control().pontia_home()
    }
    pub fn external_api_token(&self) -> Option<&str> {
        self.external_api_token.as_deref()
    }
    pub fn workspace_browser(&self) -> WorkspaceBrowserConfig {
        self.inner.workspace_browser.clone()
    }
    pub fn file_picker(&self) -> FilePickerConfig {
        self.inner.file_picker.clone()
    }
    pub fn git_refresh(&self) -> crate::GitRefreshCoordinator {
        self.inner.git_refresh.clone()
    }
    pub fn idempotency(&self) -> crate::IdempotencyCoordinator {
        self.inner.idempotency.clone()
    }
    pub fn queries(&self) -> crate::ExternalQueryService {
        crate::ExternalQueryService::new(self.db()).with_control_queries(self.control().queries())
    }
    pub fn with_external_api_token(&self, external_api_token: Option<String>) -> Self {
        Self {
            inner: self.inner.clone(),
            external_api_token,
        }
    }
}
