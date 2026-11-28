use crate::api;
use axum::{
    Router, middleware,
    routing::{get, post, put},
};
use pontia_agent_control::TurnTimelineService;
use pontia_agent_control::workspaces::WorkspaceService;
use pontia_bot::conversation::ConversationService;
use pontia_bot::identity::BotIdentity;
use pontia_bot::memory::{MemoryCommands, MemoryService};
use pontia_bot::session_messages::SessionMessages;
use pontia_bot::{
    user::{UserCommands, UserService},
    workspaces::WorkspaceCommands,
};
use std::sync::Arc;

use pontia_application::AppState;

#[derive(Clone)]
pub struct BotState {
    app: AppState,
    memory_commands: MemoryCommands,
    memories: MemoryService,
    user_service: UserService,
    user_commands: UserCommands,
    workspace_commands: WorkspaceCommands,
    session_messages: SessionMessages,
    bots: ConversationService,
    helpers: pontia_bot::helpers::HelperService,
    dreaming: pontia_bot::dreaming::DreamingService,
}

impl BotState {
    pub fn new(app: AppState, bots: ConversationService) -> Self {
        let identity = Arc::new(bots.clone());
        Self::with_identity(app, bots, identity)
    }

    pub fn with_identity(
        app: AppState,
        bots: ConversationService,
        identity: Arc<dyn BotIdentity>,
    ) -> Self {
        let helpers = pontia_bot::helpers::HelperService::new(app.control().clone(), bots.clone());
        let dreaming =
            pontia_bot::dreaming::DreamingService::new(app.control().clone(), bots.clone());
        let memories = MemoryService::new(app.db());
        let memory_commands = MemoryCommands::new(memories.clone(), identity.clone());
        let user_service = UserService::new(app.pontia_home());
        let user_commands = UserCommands::new(user_service.clone(), identity.clone());
        let workspace_commands = WorkspaceCommands::new(
            WorkspaceService::new(
                app.db(),
                app.workspace_browser(),
                app.control().control_changes(),
            ),
            identity.clone(),
        );
        let session_messages = SessionMessages::new(
            TurnTimelineService::new(app.control().event_ingest_service()),
            identity.clone(),
        );
        Self {
            session_messages,
            user_service,
            user_commands,
            workspace_commands,
            app,
            memory_commands,
            memories,
            bots,
            helpers,
            dreaming,
        }
    }

    pub fn memory_commands(&self) -> &MemoryCommands {
        &self.memory_commands
    }

    pub fn memories(&self) -> &MemoryService {
        &self.memories
    }

    pub fn user_service(&self) -> &UserService {
        &self.user_service
    }

    pub fn helpers(&self) -> &pontia_bot::helpers::HelperService {
        &self.helpers
    }

    pub fn system_notices(&self) -> pontia_bot::system_notices::SystemNoticeService {
        self.bots.system_notices()
    }

    pub fn dreaming(&self) -> &pontia_bot::dreaming::DreamingService {
        &self.dreaming
    }

    pub fn bots(&self) -> &ConversationService {
        &self.bots
    }

    pub fn user_commands(&self) -> &UserCommands {
        &self.user_commands
    }

    pub fn session_messages(&self) -> &SessionMessages {
        &self.session_messages
    }

    pub fn workspace_commands(&self) -> &WorkspaceCommands {
        &self.workspace_commands
    }

    pub fn app(&self) -> &AppState {
        &self.app
    }
}

pub fn bot_router(state: BotState) -> Router {
    Router::new()
        .route("/api/v1/bots/dreaming", post(api::bots::dream))
        .route("/api/v1/bots", get(api::bots::list).post(api::bots::create))
        .route("/api/v1/bots/clients", get(api::bots::clients))
        .route(
            "/api/v1/bots/{bot_id}",
            get(api::bots::chat)
                .put(api::bots::update)
                .delete(api::bots::delete),
        )
        .route(
            "/api/v1/bots/{bot_id}/deletion-blockers",
            get(api::bots::deletion_blockers),
        )
        .route(
            "/api/v1/bots/{bot_id}/messages",
            get(api::bots::timeline).post(api::bots::send),
        )
        .route(
            "/api/v1/bots/{bot_id}/messages/search",
            get(api::bots::search),
        )
        .route(
            "/api/v1/bots/{bot_id}/messages/{message_id}/context",
            get(api::bots::message_context),
        )
        .route(
            "/api/v1/bots/{bot_id}/messages/{message_id}/retry",
            post(api::bots::retry),
        )
        .route("/api/v1/bots/{bot_id}/reload", post(api::bots::reload))
        .route("/api/v1/bots/{bot_id}/todos", get(api::bots::todos))
        .route(
            "/api/v1/bots/{bot_id}/messages/{message_id}/todo",
            put(api::bots::mark_todo).delete(api::bots::resolve_todo),
        )
        .route("/api/v1/bots/{bot_id}/stream", get(api::bots::stream))
        .route("/api/v1/memories", get(api::list_memories))
        .route(
            "/api/v1/memory/user",
            get(api::get_user_file).put(api::replace_user_file),
        )
        .route_layer(middleware::from_fn_with_state(
            state.app().clone(),
            api::authenticate,
        ))
        .route("/api/v1/bot/session-messages", post(api::session_messages))
        .route("/api/v1/bot/chat-history", post(api::bots::chat_history))
        .route("/api/v1/bot/memory", post(api::memory_command))
        .route("/api/v1/bot/user", post(api::update_user))
        .route("/api/v1/bot/profile", post(api::profile_command))
        .route("/api/v1/bot/workspace", post(api::workspace_command))
        .route("/api/v1/bot/description", post(api::bots::set_description))
        .route("/api/v1/bot/link-message", post(api::bots::link_message))
        .route("/api/v1/bot/todo", post(api::bots::session_todo))
        .route("/api/v1/bot/send-to-user", post(api::bots::reply))
        .route("/api/v1/bot/dreaming", post(api::dreaming_command))
        .route("/api/v1/bot/helper", post(api::helper_command))
        .with_state(state)
}
