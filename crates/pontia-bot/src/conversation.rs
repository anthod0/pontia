use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use pontia_agent_control::{
    AgentControl, AgentProfileService, CreateSessionRequest, LaunchableAgentProfile,
    SessionOwnership,
};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, broadcast};

use crate::{
    handoff::HandoffService,
    identity::{BotAccess, BotIdentity, BotRole, IdentityError, TOOL_TOKEN_ENV},
    messages::{BotMessage, MessageKind, MessageService},
    records::{Bot, BotService},
    user::UserService,
};

mod status;
pub use status::WorkStatus;

mod deletion;
pub use deletion::DeletionBlockers;
mod generation;
pub use generation::GenerationLimits;
mod timeline;
pub use timeline::{NOTICE_EVENTS, TimelinePage};

const APPLICATION_ID: &str = "bot";

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationUpdate {
    Typing,
    Changed,
    Reset,
    Message { message: BotMessage },
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

    fn has_pending_input(&self) -> bool {
        !self.pending_inputs.lock().unwrap().is_empty()
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

struct ReplyInProgress<'a>(&'a Activity, Option<BotMessage>);

impl Drop for ReplyInProgress<'_> {
    fn drop(&mut self) {
        self.0.notify(
            self.1
                .take()
                .map_or(ConversationUpdate::Changed, |message| {
                    ConversationUpdate::Message { message }
                }),
        );
    }
}

#[derive(Serialize)]
pub struct Conversation {
    pub bot: Bot,
    pub messages: Vec<BotMessage>,
    pub notices: Vec<Notice>,
    pub status: WorkStatus,
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

    fn restricted_execution_policy(
        &self,
        client_type: &str,
        disable_skills: bool,
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
        policy.disable_skills = disable_skills;
        configuration.validate_execution_policy(&policy)?;
        Ok(policy)
    }

    pub fn supports_bot_client(&self, client_type: &str) -> bool {
        self.control
            .clients()
            .get(client_type)
            .is_some_and(|entry| {
                entry.spec.capabilities.steer
                    && entry.instructions.is_some()
                    && entry
                        .configuration
                        .as_ref()
                        .is_some_and(|config| config.supports_startup_model())
                    && self.restricted_execution_policy(client_type, true).is_ok()
            })
    }

    fn task_execution_policy(
        &self,
        client_type: &str,
        disable_skills: bool,
    ) -> Result<pontia_core::execution_policy::ExecutionPolicy> {
        if !self
            .control
            .clients()
            .get(client_type)
            .is_some_and(|entry| {
                entry.spec.capabilities.accept_task && entry.instructions.is_some()
            })
        {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run tasks with injected instructions".into(),
            ));
        }
        self.restricted_execution_policy(client_type, disable_skills)
    }

    /// Profiles that can currently start a helper; unavailable ones are left out of the list.
    async fn helper_profiles(&self) -> Result<Vec<LaunchableAgentProfile>> {
        let mut profiles = AgentProfileService::new(self.control.db())
            .with_clients(self.control.clients())
            .list_launchable()
            .await?;
        profiles.retain(|profile| self.require_helper_client(&profile.client_type).is_ok());
        Ok(profiles)
    }

    /// Checks the client capabilities a helper Session needs, without launching anything.
    pub(crate) fn require_helper_client(&self, client_type: &str) -> Result<()> {
        self.task_execution_policy(client_type, false)?;
        if !self
            .control
            .clients()
            .spec(client_type)
            .is_some_and(|spec| spec.capabilities.steer)
        {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run a helper Session".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn task_session_request(
        &self,
        client_type: &str,
        client_key: &str,
        model: &str,
        ownership: SessionOwnership,
        instructions: &str,
        disable_skills: bool,
    ) -> Result<CreateSessionRequest> {
        let execution_policy = self.task_execution_policy(client_type, disable_skills)?;
        let metadata = self.session_metadata(client_type, client_key).await?;
        Ok(CreateSessionRequest {
            launch_mode: Default::default(),
            execution_policy,
            model: Some(model.into()),
            client_type: client_type.into(),
            title: None,
            description: None,
            workspace: None,
            workspace_id: None,
            handle: None,
            role: None,
            system_instructions: Some(instructions.into()),
            include_standalone_session_instructions: false,
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
        self.control
            .clients()
            .creation_metadata(&self.control.db(), client_type, client_key)
            .await
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
        // Keep the reply and its status in the same snapshot.
        let _guard = activity.reply.lock().await;
        let bot = self.records.get(bot_id).await?;
        let messages = self.messages.list(bot_id).await?;
        let mut notices: Vec<_> = activity
            .generation_notice
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect();
        if let Some(session_id) = &bot.current_session_id {
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
        let status = self.work_status(&bot, &activity).await?;
        Ok(Conversation {
            bot,
            messages,
            notices,
            status,
        })
    }

    pub async fn send(&self, bot_id: &str, content: &str) -> Result<BotMessage> {
        self.start_delivery(
            bot_id,
            Input::New(crate::quote::limit_quote(content).into_owned()),
        )
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
        self.await_reply(bot_id).await?;
        activity.notify(ConversationUpdate::Message {
            message: message.clone(),
        });
        drop(recording);
        let input = format!("Source: User\n\n{}", message.content);
        let revision = activity.switch_failure.lock().unwrap().0;
        let _delivery = activity.delivery.lock().await;
        let result = self.deliver(bot_id, &input, &activity, revision).await;
        let _recording = activity.recording.lock().await;
        drop(pending);
        match result {
            Ok(()) => activity.notify(ConversationUpdate::Changed),
            Err(error) => {
                tracing::warn!(%bot_id, message_id = %message.id, %error, "Bot input delivery failed");
                let failed = self.messages.fail(&message).await?;
                activity.notify(ConversationUpdate::Message {
                    message: failed.clone(),
                });
                return Ok(failed);
            }
        }
        Ok(message)
    }

    /// Routes input by stable Bot identity, resolving the current Session at delivery time.
    pub async fn deliver_input(&self, bot_id: &str, input: &str) -> Result<()> {
        self.records.get(bot_id).await?;
        let activity = self.activity(bot_id);
        let revision = activity.switch_failure.lock().unwrap().0;
        let _guard = activity.delivery.lock().await;
        let id = uuid::Uuid::now_v7().to_string();
        let pending = activity.track_input(&id);
        self.await_reply(bot_id).await?;
        activity.notify(ConversationUpdate::Changed);
        let result = self.deliver(bot_id, input, &activity, revision).await;
        drop(pending);
        activity.notify(ConversationUpdate::Changed);
        result
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
        self.await_reply(bot_id).await?;
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
        let position = self
            .control
            .queries()
            .session_snapshot(&session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?
            .event_position;
        sqlx::query(
            "UPDATE bots SET awaiting_reply = 1, awaiting_reply_after = ? WHERE bot_id = ?",
        )
        .bind(position)
        .bind(bot_id)
        .execute(&self.control.db())
        .await?;
        let result = self
            .control
            .turn_commands()
            .submit_live_input(&session_id, input.into())
            .await;
        if result.is_err() && !matches!(&result, Err(Error::ControlUnknown(_))) {
            sqlx::query("UPDATE bots SET awaiting_reply = 0 WHERE bot_id = ?")
                .bind(bot_id)
                .execute(&self.control.db())
                .await?;
        }
        result
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
        activity.notify(ConversationUpdate::Reset);
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
                    &self.helper_profiles().await?,
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
                launch_mode: pontia_core::LaunchMode::Background,
                execution_policy: self.restricted_execution_policy(&bot.client_type, true)?,
                model: Some(bot.model.clone()),
                client_type: bot.client_type.clone(),
                title: Some(bot.name.clone()),
                description: bot.description.clone(),
                workspace: Some(directory.to_string_lossy().into_owned()),
                workspace_id: None,
                handle: None,
                role: None,
                system_instructions: Some(handoff),
                include_standalone_session_instructions: false,
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
        // Sessions of a deleted Bot remain as records but no longer act for it.
        let bot = match self.records.get(bot_id).await {
            Err(Error::NotFound(_)) => return Ok(None),
            bot => bot?,
        };
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
        if request.input.message.trim().is_empty() {
            return Err(Error::Domain("message cannot be empty".into()));
        }
        let activity = self.activity(&bot.bot_id);
        let _guard = activity.reply.lock().await;
        activity.notify(ConversationUpdate::Typing);
        let mut progress = ReplyInProgress(&activity, None);
        let message = self
            .messages
            .reply(&bot.bot_id, &request.input.message)
            .await?;
        progress.1 = Some(message.clone());
        Ok(message)
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
            if crate::helpers::helper_bot(&self.control.db(), session_id)
                .await
                .map_err(|_| IdentityError)?
                .is_some()
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
                    role: crate::helpers::helper_role(session.role.as_deref()),
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
