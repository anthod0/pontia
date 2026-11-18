use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use pontia_agent_control::{
    AgentControl, CreateSessionRequest, SessionOwnership,
    client_contract::ApplicationLaunchSettings,
};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, broadcast};

use crate::{
    handoff::HandoffService,
    identity::{BotAccess, BotIdentity, BotRole, IdentityError, TOOL_TOKEN_ENV},
    messages::{BotMessage, MessageKind, MessageService},
    records::{Bot, BotService},
    user::UserService,
};

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
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            recording: AsyncMutex::default(),
            delivery: AsyncMutex::default(),
            reply: AsyncMutex::default(),
            pending_inputs: Mutex::default(),
            updates: broadcast::channel(64).0,
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
}

#[derive(Serialize)]
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
    activity: Arc<Mutex<HashMap<String, Arc<Activity>>>>,
}

impl ConversationService {
    pub fn new(control: AgentControl) -> Self {
        let pool = control.db();
        let records = BotService::new(pool.clone(), control.pontia_home(), control.clients());
        let user = UserService::new(control.pontia_home());
        Self {
            records,
            messages: MessageService::new(pool.clone()),
            handoff: HandoffService::new(pool, user, control.queries()),
            control,
            activity: Arc::default(),
        }
    }

    pub fn records(&self) -> &BotService {
        &self.records
    }

    pub fn supports_client(&self, client_type: &str) -> bool {
        self.control
            .clients()
            .get(client_type)
            .is_some_and(|entry| {
                entry.spec.capabilities.steer
                    && entry
                        .launcher
                        .as_ref()
                        .is_some_and(|launcher| launcher.supports_application_launch())
            })
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
        let mut notices = Vec::new();
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
        Ok(Conversation {
            bot,
            messages,
            notices,
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
        let _delivery = activity.delivery.lock().await;
        let result = self.deliver(bot_id, &input, &activity).await;
        drop(pending);
        match result {
            Ok(()) => Ok(message),
            Err(error) => {
                tracing::warn!(%bot_id, message_id = %message.id, %error, "Bot input delivery failed");
                let message = self.messages.fail(&message).await?;
                activity.notify(ConversationUpdate::Changed);
                Ok(message)
            }
        }
    }

    /// Routes input by stable Bot identity, resolving the current Session at delivery time.
    pub async fn deliver_input(&self, bot_id: &str, input: &str) -> Result<()> {
        self.records.get(bot_id).await?;
        let activity = self.activity(bot_id);
        let _guard = activity.delivery.lock().await;
        self.deliver(bot_id, input, &activity).await
    }

    async fn deliver(&self, bot_id: &str, input: &str, activity: &Activity) -> Result<()> {
        let bot = self.records.get(bot_id).await?;
        let session_id = self.current_session(bot, activity).await?;
        self.control
            .turn_commands()
            .submit_live_input(&session_id, input.into())
            .await
    }

    async fn current_session(&self, bot: Bot, activity: &Activity) -> Result<String> {
        let queries = self.control.queries();
        let ownership_prefix = format!("{}:", bot.bot_id);
        let current = match &bot.current_session_id {
            Some(session_id) => Some(session_id.clone()),
            None => {
                queries
                    .latest_owned_session(APPLICATION_ID, &ownership_prefix)
                    .await?
            }
        };
        if let Some(session_id) = current {
            let snapshot = self
                .control
                .queries()
                .session_snapshot(&session_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
            if !matches!(snapshot.state.as_str(), "exited" | "error") {
                if bot.current_session_id.is_none() {
                    self.records
                        .set_current_session(&bot.bot_id, Some(&session_id))
                        .await?;
                }
                return Ok(session_id);
            }
            if snapshot.has_agent_binding {
                if snapshot.state == "exited" {
                    self.control
                        .session_commands()
                        .resume_session(&session_id, self.control.pontia_home())
                        .await?;
                    if bot.current_session_id.is_none() {
                        self.records
                            .set_current_session(&bot.bot_id, Some(&session_id))
                            .await?;
                    }
                    return Ok(session_id);
                }
                return Err(Error::StateConflict(
                    "The current Bot Session is in error".into(),
                ));
            }
        }
        if !self.supports_client(&bot.client_type) {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run a Bot Session".into(),
            ));
        }
        self.records.initialize(&bot.bot_id).await?;
        let directory = self.records.directory(&bot.bot_id).await?;
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
                .generate_for_input(&bot.bot_id, &[], &pending)
                .await
                .map_err(|error| Error::Domain(format!("Cannot prepare Bot handoff: {error}")))?
        };
        let launch = uuid::Uuid::now_v7();
        let instructions_file = directory.join(format!("handoff-{launch}.md"));
        tokio::fs::write(&instructions_file, handoff).await?;
        let outcome = self
            .control
            .session_commands()
            .create_application_session(
                CreateSessionRequest {
                    client_type: bot.client_type,
                    title: Some(bot.name),
                    description: bot.description,
                    workspace: None,
                    workspace_id: None,
                    handle: None,
                    role: None,
                    execution_profile_id: None,
                    execution_profile_version: None,
                    metadata: json!({}),
                    initial_task: None,
                    runtime_environment: [(
                        TOOL_TOKEN_ENV.into(),
                        uuid::Uuid::new_v4().to_string(),
                    )]
                    .into(),
                    ownership: Some(SessionOwnership {
                        application_id: APPLICATION_ID.into(),
                        reference: format!("{ownership_prefix}{launch}"),
                    }),
                },
                ApplicationLaunchSettings {
                    client_key: bot.client_key,
                    directory,
                    model: bot.model,
                    instructions_file,
                },
            )
            .await?;
        let session_id = outcome
            .session_id()
            .ok_or_else(|| Error::Domain("Bot Session creation returned no Session ID".into()))?;
        self.records
            .set_current_session(&bot.bot_id, Some(session_id))
            .await?;
        self.activity(&bot.bot_id)
            .notify(ConversationUpdate::Changed);
        Ok(session_id.into())
    }

    async fn bot_for_session(&self, session_id: &str) -> Result<Option<Bot>> {
        let Some(ownership) = self.control.queries().session_ownership(session_id).await? else {
            return Ok(None);
        };
        if ownership.application_id != APPLICATION_ID {
            return Ok(None);
        }
        let Some((bot_id, _)) = ownership.reference.split_once(':') else {
            return Ok(None);
        };
        let bot = self.records.get(bot_id).await?;
        Ok((bot.current_session_id.as_deref() == Some(session_id)).then_some(bot))
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
