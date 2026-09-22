use axum::extract::FromRef;

use pontia_application::AppState;

use super::dashboard::ResolvedDashboard;

#[derive(Clone)]
pub struct HttpState {
    app: AppState,
    dashboard: ResolvedDashboard,
}

impl HttpState {
    pub fn new(app: AppState, dashboard: ResolvedDashboard) -> Self {
        Self { app, dashboard }
    }

    pub fn app(&self) -> &AppState {
        &self.app
    }

    pub fn dashboard(&self) -> &ResolvedDashboard {
        &self.dashboard
    }
}

impl From<AppState> for HttpState {
    fn from(app: AppState) -> Self {
        Self::new(app, ResolvedDashboard::local_default())
    }
}

impl FromRef<HttpState> for AppState {
    fn from_ref(state: &HttpState) -> Self {
        state.app.clone()
    }
}

impl FromRef<HttpState> for ResolvedDashboard {
    fn from_ref(state: &HttpState) -> Self {
        state.dashboard.clone()
    }
}
