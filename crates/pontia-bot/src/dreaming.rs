use std::{sync::Arc, time::Duration};

use pontia_agent_control::{
    AgentControl, AgentProfileService, CreateSessionRequest, SessionOwnership,
    client_contract::ApplicationLaunchSettings,
};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, watch};

use crate::{
    conversation::ConversationService,
    identity::{BotIdentity, BotRole, TOOL_TOKEN_ENV},
    records::DREAMING_PROFILE_ID,
};

#[derive(Clone)]
pub struct DreamingService {
    control: AgentControl,
    conversation: ConversationService,
    gate: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DreamingRequest {
    pub session_id: String,
    pub command: DreamingCommand,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DreamingCommand {
    Clarify { bot_id: String, message: String },
}

impl DreamingService {
    pub fn new(control: AgentControl, conversation: ConversationService) -> Self {
        Self {
            control,
            conversation,
            gate: Arc::default(),
        }
    }

    /// Start one round; an existing active round always wins.
    pub async fn start(&self) -> Result<Option<String>> {
        let mut reported = self.gate.lock().await;
        if !self.finish_previous(&mut reported).await? {
            tracing::info!("Dreaming skipped: previous round is still active");
            return Ok(None);
        }
        let pending = self.pending_count().await?;
        if pending == 0 {
            return Ok(None);
        }
        let bots = self.conversation.records().list().await?;
        if bots.is_empty() {
            return Ok(None);
        }
        let profile = AgentProfileService::new(self.control.db())
            .with_clients(self.control.clients())
            .launch_defaults(DREAMING_PROFILE_ID, None)
            .await?;
        if !self.conversation.supports_client(&profile.client_type) {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run a Dreaming Session".into(),
            ));
        }
        let round = uuid::Uuid::now_v7().to_string();
        let directory = self
            .control
            .pontia_home()
            .join("bot")
            .join("dreaming")
            .join(&round);
        tokio::fs::create_dir_all(&directory).await?;
        let instructions_file = directory.join("instructions.md");
        tokio::fs::write(&instructions_file, include_str!("dreaming/instructions.md")).await?;
        let outcome = self
            .control
            .session_commands()
            .create_application_session(
                CreateSessionRequest {
                    execution_policy: crate::execution_policy(),
                    model: None,
                    client_type: profile.client_type,
                    title: Some("Dreaming".into()),
                    description: Some("Organize pending device memories".into()),
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
                    ownership: Some(SessionOwnership {
                        application_id: "bot".into(),
                        reference: format!("dreaming:{round}"),
                    }),
                },
                ApplicationLaunchSettings {
                    client_key: profile.client_key,
                    directory,
                    model: profile.model,
                    instructions_file,
                },
            )
            .await?;
        let session_id = outcome
            .session_id()
            .ok_or_else(|| Error::Domain("Dreaming creation returned no Session ID".into()))?;
        let recipients: Vec<_> = bots.iter().map(|bot| json!({"bot_id": bot.bot_id, "name": bot.name, "description": bot.description})).collect();
        self.control.turn_commands().submit_live_input(session_id, format!(
            "Organize all unfinished memories. Start with `pontia bot memory pending`, following every page. {pending} memories are currently pending. Available Bots for an essential clarification (choose one by context): {}", serde_json::to_string(&recipients)?
        )).await?;
        tracing::info!(%session_id, pending, "Dreaming started");
        Ok(Some(session_id.into()))
    }

    async fn pending_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM bot_memories WHERE archived = 0 AND organized = 0",
        )
        .fetch_one(&self.control.db())
        .await?)
    }

    // Session ownership survives daemon restarts; no separate run or scheduler state is needed.
    async fn finish_previous(&self, reported: &mut Option<String>) -> Result<bool> {
        let Some(id) = self
            .control
            .queries()
            .latest_owned_session("bot", "dreaming:")
            .await?
        else {
            return Ok(true);
        };
        let session = self
            .control
            .queries()
            .session_snapshot(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {id} not found")))?;
        if !matches!(
            session.state.as_str(),
            "idle" | "error" | "interrupted" | "exited"
        ) {
            return Ok(false);
        }
        if session.state == "idle" && session.has_unsettled_input {
            return Ok(false);
        }
        if reported.as_deref() != Some(&id) {
            let pending = self.pending_count().await?;
            let events = self.control.queries().list_session_events(&id).await?;
            let failures: Vec<_> = events
                .iter()
                .filter(|event| {
                    matches!(
                        event.event_type.as_str(),
                        "turn.failed" | "turn.interrupted" | "session.error" | "session.exited"
                    )
                })
                .map(|event| json!({"type": event.event_type, "details": event.payload}))
                .collect();
            tracing::info!(session_id = %id, state = %session.state, turns = ?session.turns, details = %json!(failures), pending, "Dreaming ended; unfinished memories remain for the next round");
            *reported = Some(id.clone());
        }
        if session.state == "exited" {
            return Ok(true);
        }
        self.control
            .session_commands()
            .terminate_session(&id)
            .await?;
        Ok(self
            .control
            .queries()
            .session_snapshot(&id)
            .await?
            .is_some_and(|s| s.state == "exited"))
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let hour = Duration::from_secs(3600);
        let mut hourly = tokio::time::interval_at(tokio::time::Instant::now() + hour, hour);
        hourly.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut cleanup = tokio::time::interval(Duration::from_secs(2));
        cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = hourly.tick() => {
                    if let Err(error) = self.start().await {
                        tracing::warn!(%error, "Dreaming failed; next attempt is at the next hourly tick");
                    }
                }
                _ = cleanup.tick() => {
                    let mut reported = self.gate.lock().await;
                    if let Err(error) = self.finish_previous(&mut reported).await {
                        tracing::warn!(%error, "Dreaming Session cleanup failed");
                    }
                }
            }
        }
    }

    pub async fn execute(&self, request: DreamingRequest) -> Result<Value> {
        let access = self
            .conversation
            .resolve(&request.session_id)
            .await
            .map_err(|error| Error::Conflict {
                code: "bot_session_required",
                message: error.to_string(),
            })?;
        if access.role != BotRole::Dreaming {
            return Err(Error::Conflict {
                code: "bot_session_required",
                message: "Dreaming Session required".into(),
            });
        }
        match request.command {
            DreamingCommand::Clarify { bot_id, message } => {
                if message.trim().is_empty() {
                    return Err(Error::Domain("message must not be empty".into()));
                }
                self.conversation.deliver_input(&bot_id, &format!("Source: Dreaming\nSession: {}\n\n{message}\n\nRecord any clarified facts as new memories for the next Dreaming round.", request.session_id)).await?;
                Ok(json!({"delivery": "accepted"}))
            }
        }
    }
}
