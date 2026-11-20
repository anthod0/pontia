use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use pontia_agent_control::{AgentControl, CreateSessionRequest, SessionOwnership};
use pontia_core::{Error, Result, domain::TurnState};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, broadcast};

use crate::{
    handoff::HandoffService,
    identity::{BotAccess, BotIdentity, BotRole, IdentityError, TOOL_TOKEN_ENV},
    messages::{BotMessage, MessageKind, MessageService, MessageStatus},
    records::{Bot, BotService},
    user::UserService,
};

mod generation;
pub use generation::GenerationLimits;

const APPLICATION_ID: &str = "bot";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationUpdate {
    Typing,
    Changed,
}

struct Activity {
    recording: AsyncMutex<()>,
    delivery: AsyncMutex<()>,
    reply: AsyncMutex<()>,
    pending_inputs: Mutex<HashSet<String>>,
    updates: broadcast::Sender<ConversationUpdate>,
    switch_failure: Mutex<(u64, Option<generation::SwitchFailure>)>,
    generation_notice: Mutex<Option<Notice>>,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            recording: AsyncMutex::default(),
            delivery: AsyncMutex::default(),
            reply: AsyncMutex::default(),
            pending_inputs: Mutex::default(),
            updates: broadcast::channel(64).0,
            switch_failure: Mutex::default(),
            generation_notice: Mutex::default(),
        }
    }
}

impl Activity {
    fn notify(&self, update: ConversationUpdate) {
        let _ = self.updates.send(update);
    }

    fn track_input<'a>(&'a self, id: &'a str) -> PendingInput<'a> {
        self.pending_inputs.lock().unwrap().insert(id.into());
        PendingInput { activity: self, id }
    }
}

struct PendingInput<'a> {
    activity: &'a Activity,
    id: &'a str,
}

impl Drop for PendingInput<'_> {
    fn drop(&mut self) {
        self.activity.pending_inputs.lock().unwrap().remove(self.id);
    }
}

struct ReplyInProgress<'a>(&'a Activity);

impl Drop for ReplyInProgress<'_> {
    fn drop(&mut self) {
        self.0.notify(ConversationUpdate::Changed);
    }
}

#[derive(Serialize)]
pub struct Conversation {
    pub bot: Bot,
    pub messages: Vec<BotMessage>,
    pub notices: Vec<Notice>,
    /// The user awaits a reply the Bot is receiving or working on, and no work description is set.
    pub processing: bool,
}

#[derive(Clone, Serialize)]
pub struct Notice {
    pub id: String,
    pub kind: String,
    pub created_at: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyRequest {
    pub session_id: String,
    pub input: Reply,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub message: String,
    pub working_on: Option<String>,
}

#[derive(Clone)]
pub struct ConversationService {
    control: AgentControl,
    records: BotService,
    messages: MessageService,
    handoff: HandoffService,
    default_workspace: Option<PathBuf>,
    activity: Arc<Mutex<HashMap<String, Arc<Activity>>>>,
    generation_limits: GenerationLimits,
}

impl ConversationService {
    pub fn new(
        control: AgentControl,
        user_home: Option<PathBuf>,
        roots: Vec<pontia_config::WorkspaceRootConfig>,
    ) -> Self {
        let pool = control.db();
        let records = BotService::new(pool.clone(), control.pontia_home(), control.clients());
        let user = UserService::new(control.pontia_home());
        Self {
            records,
            messages: MessageService::new(pool.clone()),
            handoff: HandoffService::new(pool, user, control.queries(), roots),
            default_workspace: user_home.map(|path| path.join("Pontia")),
            control,
            activity: Arc::default(),
            generation_limits: GenerationLimits::default(),
        }
    }

    pub(crate) fn default_workspace(&self) -> Result<&Path> {
        self.default_workspace
            .as_deref()
            .ok_or_else(|| Error::Domain("Cannot locate the user's home directory".into()))
    }

    pub fn records(&self) -> &BotService {
        &self.records
    }

    fn bot_execution_policy(
        &self,
        client_type: &str,
    ) -> Result<pontia_core::execution_policy::ExecutionPolicy> {
        let clients = self.control.clients();
        let configuration = clients
            .get(client_type)
            .and_then(|entry| entry.configuration.as_ref())
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Client cannot restrict Session tools".into())
            })?;
        let mut policy = configuration.restricted_execution_policy().ok_or_else(|| {
            Error::CapabilityUnavailable("Client cannot restrict Session tools".into())
        })?;
        policy.yolo = true;
        configuration.validate_execution_policy(&policy)?;
        Ok(policy)
    }

    pub fn supports_bot_client(&self, client_type: &str) -> bool {
        self.control
            .clients()
            .get(client_type)
            .is_some_and(|entry| {
                entry.spec.capabilities.steer
                    && entry.profile.is_some()
                    && entry
                        .configuration
                        .as_ref()
                        .is_some_and(|config| config.supports_startup_model())
                    && self.bot_execution_policy(client_type).is_ok()
            })
    }

    pub(crate) async fn profile_session_request(
        &self,
        profile: pontia_agent_control::ProfileLaunchDefaults,
        ownership: SessionOwnership,
        instructions: &str,
        execution_policy: pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<CreateSessionRequest> {
        let clients = self.control.clients();
        if !clients
            .get(&profile.client_type)
            .is_some_and(|entry| entry.spec.capabilities.accept_task && entry.profile.is_some())
        {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run tasks with an explicit Profile".into(),
            ));
        }
        let metadata = self
            .session_metadata(&profile.client_type, &profile.client_key)
            .await?;
        Ok(CreateSessionRequest {
            execution_policy,
            model: Some(profile.model),
            client_type: profile.client_type,
            title: None,
            description: None,
            workspace: None,
            workspace_id: None,
            handle: None,
            role: None,
            execution_profile_id: Some(profile.profile_id),
            execution_profile_version: Some(profile.version),
            additional_instructions: Some(instructions.into()),
            metadata,
            initial_task: None,
            runtime_environment: [(TOOL_TOKEN_ENV.into(), uuid::Uuid::new_v4().to_string())].into(),
            ownership: Some(ownership),
        })
    }

    async fn session_metadata(
        &self,
        client_type: &str,
        client_key: &str,
    ) -> Result<serde_json::Value> {
        let clients = self.control.clients();
        let configuration = clients
            .get(client_type)
            .and_then(|entry| entry.configuration.as_ref())
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Client cannot create a Bot Session".into())
            })?;
        for option in clients.creation_options(&self.control.db()).await? {
            if option.client_type == client_type
                && configuration.key(&option.metadata)? == client_key
            {
                return Ok(option.metadata);
            }
        }
        Err(Error::CapabilityUnavailable(format!(
            "Client {client_type}/{client_key} is unavailable for Session creation"
        )))
    }

    fn activity(&self, bot_id: &str) -> Arc<Activity> {
        self.activity
            .lock()
            .unwrap()
            .entry(bot_id.into())
            .or_default()
            .clone()
    }

    pub fn subscribe(&self, bot_id: &str) -> broadcast::Receiver<ConversationUpdate> {
        self.activity(bot_id).updates.subscribe()
    }

    pub async fn chat(&self, bot_id: &str) -> Result<Conversation> {
        let activity = self.activity(bot_id);
        // Serialize the snapshot with sendToUser so a reply and its work status arrive together.
        let _guard = activity.reply.lock().await;
        let bot = self.records.get(bot_id).await?;
        let messages = self.messages.list(bot_id).await?;
        let delivering = !activity.pending_inputs.lock().unwrap().is_empty();
        let unanswered = messages
            .iter()
            .rev()
            .take_while(|message| message.kind != MessageKind::BotReply)
            .any(|message| message.status == MessageStatus::Normal);
        let mut turn_active = false;
        let mut notices: Vec<_> = activity
            .generation_notice
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect();
        if let Some(session_id) = &bot.current_session_id {
            if unanswered && !delivering && bot.working_on.is_none() {
                for turn in self
                    .control
                    .queries()
                    .session_snapshot(session_id)
                    .await?
                    .map(|snapshot| snapshot.turns)
                    .unwrap_or_default()
                {
                    turn_active |= turn.state.parse::<TurnState>()?.is_active();
                }
            }
            for event in self
                .control
                .queries()
                .list_session_events(session_id)
                .await?
            {
                if matches!(
                    event.event_type.as_str(),
                    "turn.failed" | "session.error" | "session.exited"
                ) {
                    notices.push(Notice {
                        id: event.event_id,
                        kind: event.event_type,
                        created_at: event.time,
                    });
                }
            }
        }
        let processing = bot.working_on.is_none() && unanswered && (delivering || turn_active);
        Ok(Conversation {
            bot,
            messages,
            notices,
            processing,
        })
    }

    pub async fn send(&self, bot_id: &str, content: &str) -> Result<BotMessage> {
        self.start_delivery(bot_id, Input::New(content.into()))
            .await
    }

    pub async fn retry(&self, bot_id: &str, message_id: &str) -> Result<BotMessage> {
        self.start_delivery(bot_id, Input::Retry(message_id.into()))
            .await
    }

    async fn start_delivery(&self, bot_id: &str, input: Input) -> Result<BotMessage> {
        let service = self.clone();
        let bot_id = bot_id.to_owned();
        tokio::spawn(async move { service.submit(&bot_id, input).await })
            .await
            .map_err(|error| Error::Domain(format!("Bot delivery task failed: {error}")))?
    }

    async fn submit(&self, bot_id: &str, input: Input) -> Result<BotMessage> {
        self.records.get(bot_id).await?;
        let activity = self.activity(bot_id);
        let recording = activity.recording.lock().await;
        let message = match input {
            Input::New(content) => {
                self.messages
                    .append(bot_id, MessageKind::UserInput, &content)
                    .await?
            }
            Input::Retry(id) => self.messages.retry(bot_id, &id).await?,
        };
        let pending = activity.track_input(&message.id);
        drop(recording);
        activity.notify(ConversationUpdate::Changed);
        let input = format!("Source: User\n\n{}", message.content);
        let revision = activity.switch_failure.lock().unwrap().0;
        let _delivery = activity.delivery.lock().await;
        let result = self.deliver(bot_id, &input, &activity, revision).await;
        drop(pending);
        let message = match result {
            Ok(()) => message,
            Err(error) => {
                tracing::warn!(%bot_id, message_id = %message.id, %error, "Bot input delivery failed");
                self.messages.fail(&message).await?
            }
        };
        activity.notify(ConversationUpdate::Changed);
        Ok(message)
    }

    /// Routes input by stable Bot identity, resolving the current Session at delivery time.
    pub async fn deliver_input(&self, bot_id: &str, input: &str) -> Result<()> {
        self.records.get(bot_id).await?;
        let activity = self.activity(bot_id);
        let revision = activity.switch_failure.lock().unwrap().0;
        let _guard = activity.delivery.lock().await;
        self.deliver(bot_id, input, &activity, revision).await
    }

    async fn deliver(
        &self,
        bot_id: &str,
        input: &str,
        activity: &Activity,
        revision: u64,
    ) -> Result<()> {
        {
            let failure = activity.switch_failure.lock().unwrap();
            if failure.0 != revision {
                return Err(failure
                    .1
                    .as_ref()
                    .expect("failed switch has a reason")
                    .error());
            }
        }
        self.rotate(bot_id, activity, time::OffsetDateTime::now_utc())
            .await?;
        let bot = self.records.get(bot_id).await?;
        let session_id = match self.current_session(bot, activity).await {
            Ok(session) => session,
            Err(error) => {
                self.generation_error(bot_id, activity, &error);
                return Err(error);
            }
        };
        self.control
            .turn_commands()
            .submit_live_input(&session_id, input.into())
            .await
    }

    async fn current_session(&self, bot: Bot, activity: &Activity) -> Result<String> {
        if let Some(session_id) = &bot.current_session_id {
            let snapshot = self
                .control
                .queries()
                .session_snapshot(session_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
            if !matches!(snapshot.state.as_str(), "exited" | "error") {
                return Ok(session_id.clone());
            }
            if snapshot.has_agent_binding && snapshot.state == "error" {
                return Err(Error::StateConflict(
                    "The current Bot Session is in error".into(),
                ));
            }
        }
        let session = self.start_session(&bot, activity).await?;
        self.records
            .set_current_session(&bot.bot_id, Some(&session))
            .await?;
        activity.generation_notice.lock().unwrap().take();
        activity.notify(ConversationUpdate::Changed);
        Ok(session)
    }

    async fn start_session(&self, bot: &Bot, activity: &Activity) -> Result<String> {
        let ownership_prefix = format!("{}:", bot.bot_id);
        if !self.supports_bot_client(&bot.client_type) {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run a Bot Session".into(),
            ));
        }
        self.records.initialize(&bot.bot_id).await?;
        let directory = self.records.directory(&bot.bot_id).await?;
        let default_workspace =
            crate::workspaces::initialize_default(&self.control.db(), self.default_workspace()?)
                .await?;
        crate::helpers::seed_workspaces(&self.control, &bot.bot_id).await?;
        let handoff = {
            let _recording = activity.recording.lock().await;
            let pending: Vec<_> = activity
                .pending_inputs
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .collect();
            self.handoff
                .generate_for_input(
                    &bot.bot_id,
                    &crate::helpers::contexts(&self.control, &bot.bot_id).await?,
                    &pending,
                    &default_workspace.workspace_id,
                )
                .await
                .map_err(|error| Error::Domain(format!("Cannot prepare Bot handoff: {error}")))?
        };
        let launch = uuid::Uuid::now_v7();
        let outcome = self
            .control
            .session_commands()
            .create_session(CreateSessionRequest {
                execution_policy: self.bot_execution_policy(&bot.client_type)?,
                model: Some(bot.model.clone()),
                client_type: bot.client_type.clone(),
                title: Some(bot.name.clone()),
                description: bot.description.clone(),
                workspace: Some(directory.to_string_lossy().into_owned()),
                workspace_id: None,
                handle: None,
                role: None,
                execution_profile_id: None,
                execution_profile_version: None,
                additional_instructions: Some(handoff),
                metadata: self
                    .session_metadata(&bot.client_type, &bot.client_key)
                    .await?,
                initial_task: None,
                runtime_environment: [(TOOL_TOKEN_ENV.into(), uuid::Uuid::new_v4().to_string())]
                    .into(),
                ownership: Some(SessionOwnership {
                    application_id: APPLICATION_ID.into(),
                    reference: format!("{ownership_prefix}{launch}"),
                }),
            })
            .await?;
        let session_id = outcome
            .session_id()
            .ok_or_else(|| Error::Domain("Bot Session creation returned no Session ID".into()))?;
        self.control
            .session_commands()
            .await_ready(session_id)
            .await?;
        Ok(session_id.into())
    }

    async fn bot_for_session(&self, session_id: &str) -> Result<Option<Bot>> {
        let Some(ownership) = self.control.queries().session_ownership(session_id).await? else {
            return Ok(None);
        };
        if ownership.application_id != APPLICATION_ID
            || (ownership.reference.starts_with("helper:")
                || ownership.reference.starts_with("dreaming:"))
        {
            return Ok(None);
        }
        let Some((bot_id, _)) = ownership.reference.split_once(':') else {
            return Ok(None);
        };
        let bot = self.records.get(bot_id).await?;
        Ok((bot.current_session_id.as_deref() == Some(session_id)).then_some(bot))
    }

    pub(crate) async fn require_bot(&self, session_id: &str) -> Result<Bot> {
        self.bot_for_session(session_id)
            .await?
            .ok_or_else(|| Error::Conflict {
                code: "bot_session_required",
                message: IdentityError.to_string(),
            })
    }

    pub async fn send_to_user(&self, request: ReplyRequest) -> Result<BotMessage> {
        let bot = self
            .bot_for_session(&request.session_id)
            .await?
            .ok_or_else(|| Error::Conflict {
                code: "bot_session_required",
                message: IdentityError.to_string(),
            })?;
        let working_on = request
            .input
            .working_on
            .filter(|value| !value.trim().is_empty());
        if request.input.message.trim().is_empty() {
            return Err(Error::Domain("message cannot be empty".into()));
        }
        let activity = self.activity(&bot.bot_id);
        let _guard = activity.reply.lock().await;
        activity.notify(ConversationUpdate::Typing);
        let _progress = ReplyInProgress(&activity);
        self.messages
            .reply(&bot.bot_id, &request.input.message, working_on.as_deref())
            .await
    }
}

impl BotIdentity for ConversationService {
    fn authenticate_tool<'a>(
        &'a self,
        session_id: &'a str,
        token: Option<&'a str>,
    ) -> std::pin::Pin<Box<dyn Future<Output = std::result::Result<(), IdentityError>> + Send + 'a>>
    {
        Box::pin(async move {
            let token = token.ok_or(IdentityError)?;
            let authorized = self
                .control
                .queries()
                .verify_session_credential(session_id, TOOL_TOKEN_ENV, token)
                .await
                .map_err(|_| IdentityError)?;
            authorized.then_some(()).ok_or(IdentityError)
        })
    }

    fn resolve<'a>(
        &'a self,
        session_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = std::result::Result<BotAccess, IdentityError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if let Some(ownership) = self
                .control
                .queries()
                .session_ownership(session_id)
                .await
                .map_err(|_| IdentityError)?
                && ownership.application_id == APPLICATION_ID
                && ownership.reference.starts_with("dreaming:")
            {
                let session = self
                    .control
                    .queries()
                    .get_session(session_id)
                    .await
                    .map_err(|_| IdentityError)?
                    .ok_or(IdentityError)?;
                if matches!(session.state.as_str(), "exited" | "error") {
                    return Err(IdentityError);
                }
                return Ok(BotAccess {
                    role: BotRole::Dreaming,
                    workspace_id: None,
                });
            }
            if let Some(bot_id) = crate::helpers::helper_bot(&self.control.db(), session_id)
                .await
                .map_err(|_| IdentityError)?
            {
                let session = self
                    .control
                    .queries()
                    .get_session(session_id)
                    .await
                    .map_err(|_| IdentityError)?
                    .ok_or(IdentityError)?;
                if session.state == "exited" {
                    return Err(IdentityError);
                }
                return Ok(BotAccess {
                    role: crate::helpers::helper_role(&bot_id, &session),
                    workspace_id: None,
                });
            }
            self.bot_for_session(session_id)
                .await
                .map_err(|_| IdentityError)?
                .ok_or(IdentityError)?;
            Ok(BotAccess {
                role: BotRole::Bot,
                workspace_id: None,
            })
        })
    }
}

enum Input {
    New(String),
    Retry(String),
}
