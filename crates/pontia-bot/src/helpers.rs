mod notifications;

use pontia_agent_control::{
    AgentControl, SessionMessagesQuery, SessionOwnership, SessionView, SubmitInboxMessageRequest,
    TurnTimelineService, workspaces::require_active_workspace_id,
};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

use crate::{
    conversation::ConversationService, handoff::HelperContext, identity::BotRole, records::Bot,
};

const HELPER_INSTRUCTIONS: &str = include_str!("helpers/instructions.md");

#[derive(Debug)]
pub enum HelperError {
    Control(Error),
    Timeline(pontia_agent_control::TurnTimelineServiceError),
}
impl From<Error> for HelperError {
    fn from(error: Error) -> Self {
        Self::Control(error)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperRequest {
    pub session_id: String,
    pub command: HelperCommand,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum HelperCommand {
    Create(CreateHelper),
    Retrieve(RetrieveMemories),
    List,
    Send { session_id: String, message: String },
    Reply { message: String },
    Check(SessionMessagesQuery),
    Compact { session_id: String },
    Close { session_id: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrieveMemories {
    pub message: String,
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHelper {
    #[serde(flatten)]
    pub location: HelperLocation,
    pub profile_id: Option<String>,
    pub title: String,
    pub description: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperLocation {
    WorkspaceId(String),
    Temporary(String),
    #[serde(skip)]
    ReadOnly,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Delivery {
    Accepted,
    Queued,
    Failed { reason: String },
    Unknown { reason: String },
}

impl Delivery {
    fn from_result(result: Result<pontia_agent_control::LiveInputDelivery>) -> Self {
        match result {
            Ok(pontia_agent_control::LiveInputDelivery::Accepted) => Self::Accepted,
            Ok(pontia_agent_control::LiveInputDelivery::Queued) => Self::Queued,
            Err(Error::ControlUnknown(reason)) => Self::Unknown { reason },
            Err(error) => Self::Failed {
                reason: error.to_string(),
            },
        }
    }
}

#[derive(Clone)]
pub struct HelperService {
    control: AgentControl,
    conversation: ConversationService,
    reconciliation: Arc<Mutex<()>>,
    delivery_backoff: Arc<std::sync::Mutex<notifications::Backoff>>,
}

impl HelperService {
    pub fn new(control: AgentControl, conversation: ConversationService) -> Self {
        Self {
            control,
            conversation,
            reconciliation: Arc::default(),
            delivery_backoff: Arc::default(),
        }
    }

    pub async fn execute(&self, request: HelperRequest) -> std::result::Result<Value, HelperError> {
        let service = self.clone();
        tokio::spawn(async move { service.dispatch(request).await })
            .await
            .map_err(|error| Error::Domain(format!("Helper operation failed: {error}")))?
    }

    async fn dispatch(&self, request: HelperRequest) -> std::result::Result<Value, HelperError> {
        if let HelperCommand::Reply { message } = request.command {
            return self
                .reply(&request.session_id, &message)
                .await
                .map_err(Into::into);
        }
        let bot = self.conversation.require_bot(&request.session_id).await?;
        match request.command {
            HelperCommand::Retrieve(input) => {
                nonempty(&input.message)?;
                let scope = match input.workspace_id {
                    Some(workspace) => {
                        nonempty(&workspace)?;
                        format!(
                            "Requested Workspace: {workspace}. Use --workspace-id on memory reads to focus this investigation. This is a task scope, not an access restriction."
                        )
                    }
                    None => "Search all unarchived device memories as needed.".into(),
                };
                self.create(
                    &bot,
                    CreateHelper {
                        location: HelperLocation::ReadOnly,
                        profile_id: None,
                        title: "Retrieving memories".into(),
                        description: input.message.clone(),
                        message: format!("{scope}\n\n{}", input.message),
                    },
                )
                .await
                .map_err(Into::into)
            }
            HelperCommand::Create(input) => self.create(&bot, input).await.map_err(Into::into),
            HelperCommand::List => Ok(json!({"helpers": self.list(&bot.bot_id).await?})),
            HelperCommand::Send {
                session_id,
                message,
            } => {
                self.require_helper(&bot.bot_id, &session_id).await?;
                nonempty(&message)?;
                let result = self
                    .control
                    .inbox_commands()
                    .submit_live_input(&session_id, format!("Source: Bot\n\n{message}"))
                    .await;
                Ok(json!({"delivery": Delivery::from_result(result)}))
            }
            HelperCommand::Check(input) => {
                let session = self.require_helper(&bot.bot_id, &input.session_id).await?;
                let messages = TurnTimelineService::new(self.control.event_ingest_service())
                    .messages(input)
                    .await;
                match messages {
                    Ok(messages) => Ok(json!({"session": session, "messages": messages})),
                    Err(error) => {
                        use pontia_agent_control::TurnTimelineServiceError::*;
                        match error {
                            Inner(error) => Err(HelperError::Control(error)),
                            InvalidQuery(_) | SessionNotFound | TurnNotFound => {
                                Err(HelperError::Timeline(error))
                            }
                            error => Ok(
                                json!({"session": session, "messages": null, "messages_unavailable": format!("{error:?}")}),
                            ),
                        }
                    }
                }
            }
            HelperCommand::Compact { session_id } => {
                self.require_helper(&bot.bot_id, &session_id).await?;
                let accepted = self
                    .control
                    .session_commands()
                    .compact_session(&session_id)
                    .await?;
                Ok(json!(accepted))
            }
            HelperCommand::Close { session_id } => {
                self.require_helper(&bot.bot_id, &session_id).await?;
                Ok(self
                    .control
                    .session_commands()
                    .terminate_session(&session_id)
                    .await?
                    .data)
            }
            HelperCommand::Reply { .. } => unreachable!(),
        }
    }

    async fn create(&self, bot: &Bot, input: CreateHelper) -> Result<Value> {
        nonempty(&input.message)?;
        nonempty(&input.description)?;
        nonempty(&input.title)?;
        let bot_id = &bot.bot_id;
        let reference = format!("helper:{bot_id}:{}", uuid::Uuid::now_v7());
        let ownership = SessionOwnership {
            application_id: "bot".into(),
            reference: reference.clone(),
        };
        let retrieval = matches!(input.location, HelperLocation::ReadOnly);
        let profile = match &input.profile_id {
            Some(id) if !retrieval => {
                Some((id, self.control.agent_profiles().resolve_launch(id).await?))
            }
            _ => None,
        };
        let (client_type, client_key, model, reasoning_effort, instructions, mode) =
            if let Some((id, profile)) = &profile {
                let (Some(client_type), Some(client_key), Some(model)) =
                    (&profile.client_type, &profile.client_key, &profile.model)
                else {
                    return Err(Error::Domain(format!(
                        "agent profile {id} has no launch defaults"
                    )));
                };
                let instructions = [profile.system_prompt.as_deref(), Some(HELPER_INSTRUCTIONS)]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("\n\n");
                (
                    client_type,
                    client_key,
                    model,
                    &profile.reasoning_effort,
                    instructions,
                    profile.mode,
                )
            } else {
                let (instructions, mode) = if retrieval {
                    (
                        include_str!("helpers/retrieval.md"),
                        pontia_core::SessionMode::Restricted,
                    )
                } else {
                    (HELPER_INSTRUCTIONS, pontia_core::SessionMode::Default)
                };
                (
                    &bot.client_type,
                    &bot.client_key,
                    &bot.model,
                    &bot.reasoning_effort,
                    instructions.into(),
                    mode,
                )
            };
        self.conversation.require_helper_client(client_type)?;
        let mut request = self
            .conversation
            .task_session_request(
                client_type,
                client_key,
                model,
                ownership.clone(),
                &instructions,
                mode,
            )
            .await?;
        request.reasoning_effort = reasoning_effort.clone();
        if retrieval {
            request.launch_mode = pontia_core::LaunchMode::Background;
            request.role = Some(RETRIEVAL_ROLE.into());
        }
        let mut message = input.message;
        match input.location {
            HelperLocation::WorkspaceId(id) => {
                let project = require_active_workspace_id(&self.control.db(), &id).await?;
                let path = Path::new(&project.canonical_path);
                let default = self.conversation.default_workspace()?;
                if path == default
                    || default
                        .canonicalize()
                        .is_ok_and(|canonical| canonical == path)
                {
                    return Err(Error::Domain(
                        "Use --temporary NAME for the temporary Workspace".into(),
                    ));
                }
                request.workspace_id = Some(id);
            }
            HelperLocation::Temporary(name) => {
                if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                    return Err(Error::Domain(
                        "Temporary task name must be a single directory name".into(),
                    ));
                }
                let path = self.conversation.default_workspace()?;
                let workspace =
                    crate::workspaces::initialize_default(&self.control.db(), path).await?;
                let directory = Path::new(&workspace.canonical_path).join(name);
                match tokio::fs::create_dir(&directory).await {
                    Ok(()) => {}
                    Err(error)
                        if error.kind() == std::io::ErrorKind::AlreadyExists
                            && directory.is_dir() => {}
                    Err(error) => return Err(error.into()),
                }
                message = format!(
                    "Work in {} and keep all files for this task in that directory.\n\n{message}",
                    directory.display()
                );
                request.workspace_id = Some(workspace.workspace_id);
            }
            HelperLocation::ReadOnly => {
                let directory = self.conversation.records().read_only_directory().await?;
                request.workspace = Some(directory.to_string_lossy().into_owned());
            }
        }
        request.title = Some(input.title);
        request.description = Some(input.description);
        // Save the unique creation reference first so recovery can find a Session even if launch fails.
        sqlx::query("INSERT INTO bot_helpers(reference, bot_id) VALUES (?, ?)")
            .bind(&reference)
            .bind(bot_id)
            .execute(&self.control.db())
            .await?;
        let outcome = self
            .control
            .session_commands()
            .create_session(request)
            .await;
        if let Some(session) = self
            .control
            .queries()
            .find_session_by_ownership(&ownership)
            .await?
        {
            associate(&self.control.db(), &reference, &session).await?;
        } else if outcome.is_err() {
            // Validation can reject a launch before it creates any Session to recover.
            sqlx::query("DELETE FROM bot_helpers WHERE reference = ? AND session_id IS NULL")
                .bind(&reference)
                .execute(&self.control.db())
                .await?;
        }
        let outcome = outcome?;
        let session_id = outcome
            .session_id()
            .ok_or_else(|| Error::Domain("Helper creation returned no Session ID".into()))?;
        let message = format!("Source: Bot\n\n{message}");
        let delivery = if retrieval {
            async {
                let outcome = self
                    .control
                    .inbox_commands()
                    .submit_message(
                        session_id,
                        SubmitInboxMessageRequest {
                            input: message,
                            delivery_policy: "after_idle".into(),
                            metadata: json!({}),
                            branch_target_turn_id: None,
                        },
                    )
                    .await?;
                self.control
                    .session_commands()
                    .exit_when_done(session_id)
                    .await?;
                outcome.live_delivery()
            }
            .await
        } else {
            self.control
                .inbox_commands()
                .submit_live_input(session_id, message)
                .await
        };
        Ok(json!({"session_id": session_id, "delivery": Delivery::from_result(delivery)}))
    }

    async fn reply(&self, source: &str, message: &str) -> Result<Value> {
        nonempty(message)?;
        let bot_id = helper_bot(&self.control.db(), source)
            .await?
            .ok_or_else(forbidden)?;
        let session = self.require_helper(&bot_id, source).await?;
        if session.state == "exited" {
            return Err(forbidden());
        }
        let result = self
            .conversation
            .deliver_input(
                &bot_id,
                &format!("Source: Helper\nSession: {source}\n\n{message}"),
            )
            .await;
        let delivery = Delivery::from_result(result);
        if matches!(delivery, Delivery::Unknown { .. } | Delivery::Failed { .. }) {
            self.enqueue(&format!("reply:{}", uuid::Uuid::now_v7()), &bot_id, Some(source),
                &format!("Helper reply delivery result: {}. This describes delivery, not helper execution. The original reply will not be resent.", serde_json::to_string(&delivery)?)).await?;
        }
        Ok(json!({"delivery": delivery}))
    }

    async fn require_helper(&self, bot_id: &str, session_id: &str) -> Result<SessionView> {
        if helper_bot(&self.control.db(), session_id).await?.as_deref() != Some(bot_id) {
            return Err(forbidden());
        }
        self.control
            .queries()
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))
    }

    /// Helper Sessions that have not exited, including memory retrieval helpers.
    pub async fn list(&self, bot_id: &str) -> Result<Vec<SessionView>> {
        open_sessions(&self.control, bot_id).await
    }
}

/// Helper Sessions of a Bot that have not exited.
pub(crate) async fn open_sessions(
    control: &AgentControl,
    bot_id: &str,
) -> Result<Vec<SessionView>> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT session_id FROM bot_helpers WHERE bot_id = ? AND session_id IS NOT NULL ORDER BY session_id")
        .bind(bot_id).fetch_all(&control.db()).await?;
    let mut sessions = Vec::new();
    for id in ids {
        let session = control
            .queries()
            .get_session(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {id} not found")))?;
        if session.state != "exited" {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

pub(crate) async fn helper_bot(pool: &SqlitePool, session_id: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT bot_id FROM bot_helpers WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await?,
    )
}

async fn associate(pool: &SqlitePool, reference: &str, session: &str) -> Result<()> {
    sqlx::query("UPDATE bot_helpers SET session_id = ? WHERE reference = ?")
        .bind(session)
        .bind(reference)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn contexts(control: &AgentControl, bot_id: &str) -> Result<Vec<HelperContext>> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT session_id FROM bot_helpers WHERE bot_id = ? AND session_id IS NOT NULL ORDER BY session_id")
        .bind(bot_id).fetch_all(&control.db()).await?;
    let mut helpers = Vec::new();
    for id in ids {
        let session = control
            .queries()
            .get_session(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {id} not found")))?;
        helpers.push(HelperContext {
            bot_id: bot_id.into(),
            session_id: id,
            role: helper_role(session.role.as_deref()),
            workspace_id: session.workspace_id,
            description: session.description,
            state: session
                .state
                .parse()
                .map_err(|_| Error::Domain("Invalid Session state".into()))?,
        });
    }
    Ok(helpers)
}

const RETRIEVAL_ROLE: &str = "retrieval";

pub(crate) fn helper_role(role: Option<&str>) -> BotRole {
    if role == Some(RETRIEVAL_ROLE) {
        BotRole::Retrieval
    } else {
        BotRole::Helper
    }
}

fn forbidden() -> Error {
    Error::Conflict {
        code: "bot_session_required",
        message: "Session is not authorized for this helper operation".into(),
    }
}

fn nonempty(message: &str) -> Result<()> {
    if message.trim().is_empty() {
        return Err(Error::Domain(
            "message, description, and title must not be empty".into(),
        ));
    }
    Ok(())
}
