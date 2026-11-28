use axum::Router;
use pontia_application::AppState;
use pontia_bot::conversation::ConversationService;
use pontia_http::{BotState, HttpEntrypoints, HttpState, bot_router, core_router, workflow_router};

pub(crate) fn conversations(app: &AppState) -> ConversationService {
    ConversationService::new(
        app.control().clone(),
        Some(app.pontia_home().to_owned()),
        app.workspace_browser().roots,
    )
}

pub(crate) fn entrypoints(state: impl Into<HttpState>) -> HttpEntrypoints {
    let state = state.into();
    let app = state.app();
    let bot = BotState::new(app.clone(), conversations(app));
    let external_api = core_router(app.clone())
        .merge(bot_router(bot))
        .merge(workflow_router(app.clone()));
    HttpEntrypoints::new(state, external_api)
}

pub(crate) fn router(state: impl Into<HttpState>) -> Router {
    entrypoints(state).local_http()
}
