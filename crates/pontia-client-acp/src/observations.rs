use crate::{
    peer::{Peer, fact, runtime_event},
    relay::{Observation, Report},
};
use pontia_agent_control::{AgentBindingService, PontiaEventType, UpsertAgentBindingRequest};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::json;

impl Peer {
    pub(crate) async fn apply_report(&self, report: Report) -> Result<()> {
        let events = &self.events;
        let session = &self.session;
        let runtime = &self.runtime;
        match report.observation {
            Observation::PromotionPending { native } => {
                let mut state = self.state.lock().await;
                state.promoted = Some(native);
                state.refresh_capabilities();
                self.remember(&state, None).await?;
            }
            Observation::PromotionStarted {
                native,
                digest,
                summary,
                index,
            } => {
                let key = format!("acp:promoted:{native}");
                let result = fact(
                    events,
                    session,
                    runtime,
                    None,
                    EventType::TurnStarted,
                    key,
                    json!({"acp_history_anchor":{"prompt":index},"input":{"summary":summary}}),
                )
                .await?;
                let turn = result
                    .turn_id
                    .ok_or_else(|| Error::Domain("Missing observed Turn identity".into()))?;
                let mut state = self.state.lock().await;
                let mut candidates = state
                    .interjections
                    .values()
                    .filter(|entry| entry.digest == digest);
                if let Some(entry) = candidates.next()
                    && candidates.next().is_none()
                    && let Some(message) = &entry.message
                {
                    pontia_storage_sqlite::repositories::inbox::SqliteInboxRepository::new(
                        events.db(),
                    )
                    .link_started_turn(session, message, &turn)
                    .await?;
                }
                state.active = Some(crate::peer::Prompt {
                    turn,
                    prompt_index: Some(index),
                    request_id: serde_json::Value::Null,
                    started: true,
                    sequence: 0,
                    raw_results: Default::default(),
                    thought: None,
                    cancelled: false,
                    history_index: 0,
                    history_end: 0,
                });
                state.promoted = Some(native);
                state.refresh_capabilities();
                self.remember(&state, None).await?;
            }
            Observation::PromotionFinished {
                native,
                reason,
                index,
            } => {
                let turn: String = sqlx::query_scalar("SELECT turn_id FROM events WHERE session_id=? AND event_type='turn.started' AND report_key=?")
                    .bind(session).bind(format!("acp:promoted:{native}")).fetch_one(&events.db()).await?;
                self.state.lock().await.promoted = None;
                self.finish_turn(turn, reason, json!({"prompt":index+1}))
                    .await?;
            }
            Observation::Setup {
                binding,
                metadata,
                model,
                cwd,
            } => {
                AgentBindingService::new(events.db())
                    .upsert_binding(UpsertAgentBindingRequest {
                        session_id: session.clone(),
                        client_type: "acp".into(),
                        launch_cwd: cwd.clone(),
                        client_session_key: binding.clone(),
                        client_session_file: None,
                        metadata: metadata.clone(),
                    })
                    .await?;
                self.relay
                    .call("relay.bind", json!({"client_session_key":binding}))
                    .await?;
                if let Some(model) = model {
                    crate::models::report(events, session, runtime, &model, report.key.clone())
                        .await?;
                }
                let running: bool = sqlx::query_scalar(
                    "SELECT state='running' FROM session_runtimes WHERE runtime_id=?",
                )
                .bind(runtime)
                .fetch_one(&events.db())
                .await?;
                if !running {
                    runtime_event(
                        events,
                        session,
                        runtime,
                        PontiaEventType::RuntimeReady,
                        json!({}),
                    )
                    .await?;
                }
                fact(
                    events, session, runtime, None, EventType::SessionReady, report.key,
                    json!({"client_session_key": binding, "launch_cwd": cwd, "binding_metadata": metadata}),
                ).await?;
            }
            Observation::AutoCompactionStarted { operation } => {
                fact(
                    events,
                    session,
                    runtime,
                    None,
                    EventType::SessionCompactionStarted,
                    format!("acp:compaction_started:{operation}"),
                    json!({"operation_id":operation,"trigger":"unknown"}),
                )
                .await?;
            }
            Observation::Compacted {
                operation,
                manual,
                tokens_before,
                tokens_after,
            } => {
                fact(
                    events,
                    session,
                    runtime,
                    None,
                    EventType::SessionCompactionCompleted,
                    format!("acp:compacted:{operation}"),
                    json!({"operation_id":operation,"trigger":if manual {"manual"} else {"unknown"},
                        "tokens_before":tokens_before,"tokens_after":tokens_after}),
                )
                .await?;
                if manual {
                    self.finish_compaction(&operation).await?;
                }
            }
            Observation::CompactionFailed {
                operation,
                manual,
                aborted,
                message,
            } => {
                fact(events, session, runtime, None, EventType::SessionCompactionFailed,
                    format!("acp:compaction_failed:{operation}"),
                    json!({"operation_id":operation,"trigger":if manual {"manual"} else {"unknown"},"aborted":aborted,"error_message":message})).await?;
                if manual {
                    self.finish_compaction(&operation).await?;
                }
            }
            Observation::Usage { used, size } => {
                if size == 0 {
                    return Err(Error::Domain("ACP usage capacity is zero".into()));
                }
                let usage = json!({
                    "used_tokens": used,
                    "max_tokens": size,
                    "remaining_tokens": size.saturating_sub(used),
                    "usage_ratio": (used as f64 / size as f64).min(1.0),
                    "confidence": "exact",
                });
                fact(
                    events,
                    session,
                    runtime,
                    None,
                    EventType::SessionContextUsageUpdated,
                    report.key,
                    json!({"context_usage": usage}),
                )
                .await?;
            }
            Observation::UsageEstimate { ratio, size } => {
                let used = size.map(|size| (ratio * size as f64).round() as u64);
                fact(events, session, runtime, None, EventType::SessionContextUsageUpdated,
                    report.key, json!({"context_usage":{
                        "usage_ratio": ratio, "confidence": "estimated", "max_tokens": size,
                        "used_tokens": used, "remaining_tokens": size.zip(used).map(|(size, used)| size - used)
                    }})).await?;
            }
            Observation::Model { model } => {
                crate::models::report(events, session, runtime, &model, report.key).await?;
            }
            Observation::Started { turn, anchor } => {
                fact(
                    events,
                    session,
                    runtime,
                    Some(&turn),
                    EventType::TurnStarted,
                    format!("acp:{turn}:turn.started"),
                    json!({"acp_history_anchor":anchor}),
                )
                .await?;
                if let Some(prompt) = self
                    .state
                    .lock()
                    .await
                    .active
                    .as_mut()
                    .filter(|p| p.turn == turn)
                {
                    prompt.started = true;
                }
            }
            Observation::Finished {
                turn,
                anchor,
                reason,
            } => {
                self.finish_turn(turn, reason, anchor).await?;
            }
            Observation::Delivered {
                registration,
                digest,
            } => {
                self.interactions
                    .settle(session, &registration, &digest, &report.key, "delivered")
                    .await?;
            }
            Observation::Cancelled {
                registration,
                digest,
            } => {
                if let Some((stored, original)) = self
                    .interactions
                    .registration(session, &registration)
                    .await?
                {
                    if digest != original {
                        return Err(Error::StateConflict(
                            "ACP cancellation identity changed".into(),
                        ));
                    }
                    self.interactions
                        .expire(session, &stored.interaction_id)
                        .await?;
                    self.state
                        .lock()
                        .await
                        .permissions
                        .remove(&stored.interaction_id);
                }
            }
            Observation::Permission { .. } => {
                return Err(Error::Domain(
                    "Permission requires its native responder".into(),
                ));
            }
        }
        Ok(())
    }

    async fn finish_turn(
        &self,
        turn: String,
        reason: String,
        anchor: serde_json::Value,
    ) -> Result<()> {
        let events = &self.events;
        let session = &self.session;
        let runtime = &self.runtime;
        let kind = match reason.as_str() {
            "end_turn" => EventType::TurnCompleted,
            "cancelled" => EventType::TurnInterrupted,
            "max_tokens" | "max_turn_requests" | "refusal" => EventType::TurnFailed,
            _ => {
                return Err(Error::Domain(format!(
                    "Unknown ACP prompt stop reason: {reason}"
                )));
            }
        };
        let mut data = json!({"acp_history_anchor":anchor});
        if kind == EventType::TurnFailed {
            data["failure"] = json!({"message":format!("ACP prompt stopped: {reason}")});
        }
        fact(
            events,
            session,
            runtime,
            Some(&turn),
            kind,
            format!("acp:{turn}:{kind}"),
            data,
        )
        .await?;
        let mut state = self.state.lock().await;
        if state.active.as_ref().is_some_and(|p| p.turn == turn) {
            state.active = None;
            self.remember(&state, None).await?;
            let permissions = std::mem::take(&mut state.permissions);
            state.refresh_capabilities();
            drop(state);
            crate::interactions::expire(&self.interactions, session, permissions, None).await?;
        }
        Ok(())
    }
}
