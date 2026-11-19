mod notifications;
pub(crate) use notifications::seed_workspaces;

use pontia_agent_control::{
    AgentControl, AgentProfileService, CreateSessionRequest, SessionMessagesQuery,
    SessionOwnership, SessionView, TurnTimelineService, client_contract::ApplicationLaunchSettings,
};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    conversation::ConversationService,
    handoff::HelperContext,
    identity::{BotRole, TOOL_TOKEN_ENV},
    records::BotService,
};

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
    Close { session_id: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrieveMemories {
    pub message: String,
    pub workspace_id: Option<String>,
    pub profile_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHelper {
    pub profile_id: Option<String>,
    pub profile_version: Option<String>,
    pub description: String,
    pub message: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Delivery {
    Accepted,
    Failed { reason: String },
    Unknown { reason: String },
}

impl Delivery {
    fn from_result(result: Result<()>) -> Self {
        match result {
            Ok(()) => Self::Accepted,
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
}

impl HelperService {
    pub fn new(control: AgentControl, conversation: ConversationService) -> Self {
        Self {
            control,
            conversation,
            reconciliation: Arc::default(),
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
                self.conversation.records().initialize(&bot.bot_id).await?;
                self.create(
                    &bot.bot_id,
                    &request.session_id,
                    CreateHelper {
                        profile_id: Some(BotService::retrieval_profile_id(&bot.bot_id)),
                        profile_version: input.profile_version,
                        description: input.message.clone(),
                        message: format!("{scope}\n\n{}", input.message),
                    },
                )
                .await
                .map_err(Into::into)
            }
            HelperCommand::Create(input) => self
                .create(&bot.bot_id, &request.session_id, input)
                .await
                .map_err(Into::into),
            HelperCommand::List => Ok(json!({"helpers": self.list(&bot.bot_id).await?})),
            HelperCommand::Send {
                session_id,
                message,
            } => {
                self.require_helper(&bot.bot_id, &session_id).await?;
                nonempty(&message)?;
                let result = self
                    .control
                    .turn_commands()
                    .submit_live_input(
                        &session_id,
                        format!("Source: Bot\nSession: {}\n\n{message}", request.session_id),
                    )
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

    async fn create(&self, bot_id: &str, source: &str, input: CreateHelper) -> Result<Value> {
        nonempty(&input.message)?;
        nonempty(&input.description)?;
        let profile_id = input
            .profile_id
            .unwrap_or_else(|| BotService::helper_profile_id(bot_id));
        let profile = AgentProfileService::new(self.control.db())
            .with_clients(self.control.clients())
            .launch_defaults(&profile_id, input.profile_version.as_deref())
            .await?;
        if !self.conversation.supports_client(&profile.client_type) {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run a helper Session".into(),
            ));
        }
        let reference = format!("helper:{bot_id}:{}", uuid::Uuid::now_v7());
        let directory = self
            .conversation
            .records()
            .directory(bot_id)
            .await?
            .join("helpers")
            .join(reference.rsplit(':').next().unwrap());
        tokio::fs::create_dir_all(&directory).await?;
        let instructions_file = directory.join("instructions.md");
        tokio::fs::write(&instructions_file, include_str!("helpers/instructions.md")).await?;
        // Save the unique creation reference first so recovery can find a Session even if launch fails.
        sqlx::query("INSERT INTO bot_helpers(reference, bot_id) VALUES (?, ?)")
            .bind(&reference)
            .bind(bot_id)
            .execute(&self.control.db())
            .await?;
        let ownership = SessionOwnership {
            application_id: "bot".into(),
            reference: reference.clone(),
        };
        let outcome = self
            .control
            .session_commands()
            .create_application_session(
                CreateSessionRequest {
                    execution_policy: crate::execution_policy(),
                    model: None,
                    client_type: profile.client_type,
                    title: None,
                    description: Some(input.description),
                    workspace: None,
                    workspace_id: None,
                    handle: None,
                    role: None,
                    execution_profile_id: Some(profile.profile_id),
                    execution_profile_version: Some(profile.version),
                    additional_instructions: None,
                    metadata: json!({}),
                    initial_task: None,
                    runtime_environment: [(
                        TOOL_TOKEN_ENV.into(),
                        uuid::Uuid::new_v4().to_string(),
                    )]
                    .into(),
                    ownership: Some(ownership.clone()),
                },
                ApplicationLaunchSettings {
                    client_key: profile.client_key,
                    directory,
                    model: profile.model,
                    instructions_file,
                },
            )
            .await;
        if let Some(session) = self
            .control
            .queries()
            .find_session_by_ownership(&ownership)
            .await?
        {
            associate(&self.control.db(), &reference, &session).await?;
        }
        let outcome = outcome?;
        let session_id = outcome
            .session_id()
            .ok_or_else(|| Error::Domain("Helper creation returned no Session ID".into()))?;
        let delivery = self
            .control
            .turn_commands()
            .submit_live_input(
                session_id,
                format!("Source: Bot\nSession: {source}\n\n{}", input.message),
            )
            .await;
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
        if delivery != Delivery::Accepted {
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

    pub async fn list(&self, bot_id: &str) -> Result<Vec<SessionView>> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT session_id FROM bot_helpers WHERE bot_id = ? AND session_id IS NOT NULL ORDER BY session_id")
            .bind(bot_id).fetch_all(&self.control.db()).await?;
        let mut sessions = Vec::new();
        for id in ids {
            let session = self.require_helper(bot_id, &id).await?;
            if session.state != "exited" {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }
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
            role: helper_role(bot_id, &session),
            description: session.description,
            state: session
                .state
                .parse()
                .map_err(|_| Error::Domain("Invalid Session state".into()))?,
        });
    }
    Ok(helpers)
}

pub(crate) fn helper_role(bot_id: &str, session: &SessionView) -> BotRole {
    if session.execution_profile_id.as_deref()
        == Some(BotService::retrieval_profile_id(bot_id).as_str())
    {
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
            "message and description must not be empty".into(),
        ));
    }
    Ok(())
}
