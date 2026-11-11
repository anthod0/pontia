use super::AppState;
use pontia_agent_control::AgentControl;
use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig};
pub struct AppStateBuilder {
    pub(super) control: AgentControl,
    pub(super) external_api_token: Option<String>,
    pub(super) workspace_browser: WorkspaceBrowserConfig,
    pub(super) file_picker: FilePickerConfig,
    pub(super) git_refresh: crate::GitRefreshCoordinator,
    pub(super) idempotency: crate::IdempotencyCoordinator,
}
impl AppStateBuilder {
    pub(super) fn new(control: AgentControl) -> Self {
        Self {
            control,
            external_api_token: None,
            workspace_browser: Default::default(),
            file_picker: Default::default(),
            git_refresh: Default::default(),
            idempotency: Default::default(),
        }
    }
    pub fn external_api_token(mut self, token: Option<String>) -> Self {
        self.external_api_token = token;
        self
    }
    pub fn workspace_browser(mut self, config: WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = config;
        self
    }
    pub fn file_picker(mut self, config: FilePickerConfig) -> Self {
        self.file_picker = config;
        self
    }
    pub fn git_refresh(mut self, coordinator: crate::GitRefreshCoordinator) -> Self {
        self.git_refresh = coordinator;
        self
    }
    pub fn idempotency(mut self, coordinator: crate::IdempotencyCoordinator) -> Self {
        self.idempotency = coordinator;
        self
    }
    pub fn build(self) -> AppState {
        AppState::from_builder(self)
    }
}
