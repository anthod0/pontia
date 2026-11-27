use std::{sync::Arc, time::Duration};

use pontia_agent_control::{
    AgentControl, SessionOwnership, SubmitInboxMessageRequest,
    interactions::{InteractionIntent, InteractionState},
};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, watch};

use crate::{
    conversation::ConversationService,
    identity::{BotIdentity, BotRole},
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
        let Some(bot) = bots.first() else {
            return Ok(None);
        };
        if !self
            .control
            .clients()
            .spec(&bot.client_type)
            .is_some_and(|spec| {
                spec.capabilities.report_turn_started && spec.capabilities.report_turn_finished
            })
        {
            return Err(Error::CapabilityUnavailable(
                "Client cannot run a Dreaming Session".into(),
            ));
        }
        let round = uuid::Uuid::now_v7().to_string();
        let mut request = self
            .conversation
            .task_session_request(
                &bot.client_type,
                &bot.client_key,
                &bot.model,
                SessionOwnership {
                    application_id: "bot".into(),
                    reference: format!("dreaming:{round}"),
                },
                include_str!("dreaming/instructions.md"),
                pontia_core::SessionMode::Restricted,
            )
            .await?;
        let directory = self.conversation.records().read_only_directory().await?;
        request.reasoning_effort = bot.reasoning_effort.clone();
        request.launch_mode = pontia_core::LaunchMode::Background;
        request.workspace = Some(directory.to_string_lossy().into_owned());
        request.title = Some("Dreaming".into());
        request.description = Some("Organize pending device memories".into());
        let outcome = self
            .control
            .session_commands()
            .create_session(request)
            .await?;
        let session_id = outcome
            .session_id()
            .ok_or_else(|| Error::Domain("Dreaming creation returned no Session ID".into()))?;
        let recipients: Vec<_> = bots
            .iter()
            .map(|bot| {
                json!({
                    "bot_id": bot.bot_id, "name": bot.name, "description": bot.description
                })
            })
            .collect();
        let input = format!(
            "Organize all unfinished memories. Start with `pontia bot memory pending`, following every page. {pending} memories are currently pending. Available Bots for an essential clarification (choose one by context): {}",
            serde_json::to_string(&recipients)?
        );
        // The Inbox receipt keeps this round in flight until the native Turn is observed.
        let delivery = self
            .control
            .inbox_commands()
            .submit_message(
                session_id,
                SubmitInboxMessageRequest {
                    input,
                    delivery_policy: "after_idle".into(),
                    metadata: json!({}),
                },
            )
            .await?;
        self.control
            .session_commands()
            .exit_when_done(session_id)
            .await?;
        let message = &delivery.data["inbox_message"];
        if let Some(reason) = message["failure_message"].as_str() {
            if message["state"] == "unknown" {
                return Err(Error::ControlUnknown(reason.into()));
            }
            return Err(Error::Domain(reason.into()));
        }
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
        let Some(previous) = self
            .control
            .queries()
            .latest_owned_session("bot", "dreaming:")
            .await?
        else {
            return Ok(true);
        };
        let id = previous.session_id;
        let session = self
            .control
            .queries()
            .session_snapshot(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {id} not found")))?;
        let exited = match session.state.as_str() {
            "exited" => true,
            "created" | "starting" if session.has_unsettled_input => false,
            "waiting_for_interaction" => {
                self.reject_blocked_interactions(&id).await;
                self.control
                    .session_commands()
                    .terminate_session(&id)
                    .await?;
                self.control
                    .queries()
                    .session_snapshot(&id)
                    .await?
                    .is_some_and(|session| session.state == "exited")
            }
            "created" | "starting" | "interrupted" | "error" => {
                self.control
                    .session_commands()
                    .terminate_session(&id)
                    .await?;
                self.control
                    .queries()
                    .session_snapshot(&id)
                    .await?
                    .is_some_and(|session| session.state == "exited")
            }
            _ => false,
        };
        if !exited {
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
                        "turn.failed"
                            | "turn.interrupted"
                            | "session.error"
                            | "session.exited"
                            | "interaction.blocked"
                    )
                })
                .map(|event| json!({"type": event.event_type, "details": event.payload}))
                .collect();
            tracing::info!(session_id = %id, state = %session.state, turns = ?session.turns, details = %json!(failures), pending, "Dreaming ended; unfinished memories remain for the next round");
            *reported = Some(id.clone());
        }
        Ok(true)
    }

    async fn reject_blocked_interactions(&self, session_id: &str) {
        let interactions = self.control.interactions();
        let mut cursor = None;
        let mut seen = Vec::new();
        loop {
            let page = match interactions
                .list(
                    session_id,
                    Some(InteractionState::Pending),
                    200,
                    cursor.as_deref(),
                )
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    tracing::warn!(
                        %session_id,
                        %error,
                        "Dreaming Session blocked on interaction; failed to list pending interactions before terminating"
                    );
                    return;
                }
            };
            for interaction in page.interactions {
                seen.push(interaction.interaction_id.clone());
                let Some(option) = interaction
                    .options
                    .iter()
                    .find(|option| option.intent == InteractionIntent::Negative)
                else {
                    tracing::warn!(
                        %session_id,
                        interaction_id = %interaction.interaction_id,
                        title = %interaction.title,
                        "Dreaming Session blocked on interaction without a negative option; terminating without responding"
                    );
                    continue;
                };
                match interactions
                    .respond(
                        &self.control,
                        session_id,
                        &interaction.interaction_id,
                        &option.key,
                    )
                    .await
                {
                    Ok(_) => {
                        tracing::warn!(
                            %session_id,
                            interaction_id = %interaction.interaction_id,
                            option_key = %option.key,
                            title = %interaction.title,
                            "Dreaming Session blocked on interaction; rejected pending interaction before terminating"
                        );
                    }
                    Err(error) => {
                        tracing::warn!(
                            %session_id,
                            interaction_id = %interaction.interaction_id,
                            option_key = %option.key,
                            %error,
                            "Dreaming Session blocked on interaction; failed to reject pending interaction before terminating"
                        );
                    }
                }
            }
            let Some(next) = page.next_cursor else {
                break;
            };
            cursor = Some(next);
        }
        if seen.is_empty() {
            tracing::warn!(
                %session_id,
                "Dreaming Session blocked on interaction; terminating Session"
            );
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let hour = Duration::from_secs(3600);
        let mut hourly = tokio::time::interval_at(tokio::time::Instant::now() + hour, hour);
        hourly.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = hourly.tick() => {
                    if let Err(error) = self.start().await {
                        tracing::warn!(%error, "Dreaming failed; next attempt is at the next hourly tick");
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
                let delivery = self.conversation.deliver_input(&bot_id, &format!("Source: Dreaming\nSession: {}\n\n{message}\n\nRecord any clarified facts as new memories for the next Dreaming round.", request.session_id)).await?;
                Ok(json!({"delivery": delivery}))
            }
        }
    }
}
