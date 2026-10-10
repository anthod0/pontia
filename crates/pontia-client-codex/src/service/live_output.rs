use super::{CodexService, string};
use crate::runtime::{CodexRuntime, SubscriptionState, protocol::Notification};
use pontia_application::{
    AgentBindingService, LiveOutputBatch, LiveOutputIdentity, LiveOutputItem, LiveOutputProducer,
    LiveOutputPublishOutcome, LiveOutputService, LiveOutputSnapshotReplacement, LiveOutputSource,
    LiveOutputUpdate,
};
use pontia_core::Result;
use std::collections::{HashMap, HashSet};

enum Recovery {
    Ready {
        turn_id: String,
        boundary: u64,
        items: HashSet<String>,
    },
    Unavailable {
        turn_id: String,
    },
}

pub(super) struct CodexLiveOutput {
    output: LiveOutputService,
    recovered: HashMap<String, Recovery>,
}

impl Drop for CodexLiveOutput {
    fn drop(&mut self) {
        for session in self.recovered.keys() {
            self.output.discard_session(session);
        }
    }
}

impl CodexLiveOutput {
    pub fn new(output: LiveOutputService) -> Self {
        Self {
            output,
            recovered: HashMap::new(),
        }
    }

    pub fn invalidate(&mut self, session: &str) {
        self.recovered.remove(session);
        self.output.discard_session(session);
    }

    pub async fn reconcile(
        &mut self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        thread: &str,
    ) {
        if let Err(error) = self.restore(service, runtime, session, thread).await {
            self.invalidate(session);
            tracing::warn!(%session, %error, "Codex live output recovery unavailable");
        }
    }

    fn unavailable(&mut self, session: &str, native: &str) {
        self.invalidate(session);
        self.recovered.insert(
            session.into(),
            Recovery::Unavailable {
                turn_id: native.into(),
            },
        );
    }

    pub async fn restore(
        &mut self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        thread: &str,
    ) -> Result<()> {
        {
            let _current = runtime.current_guard().await?;
            if !self.accepts(service, runtime, session, thread).await? {
                return Ok(());
            }
        }
        let connection = runtime.connection().await?;
        let turns = service.observed_turns(&connection, thread).await?;
        let Some(turns) = turns else { return Ok(()) };
        service
            .reconcile_turns(
                session,
                runtime,
                &turns
                    .iter()
                    .map(|snapshot| snapshot.turn.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        let _current = runtime.current_guard().await?;
        if !self.accepts(service, runtime, session, thread).await? {
            return Ok(());
        }
        for snapshot in turns
            .iter()
            .filter(|snapshot| snapshot.turn["status"] == "inProgress")
        {
            let turn = &snapshot.turn;
            let native = string(turn, "id")?;
            let Some(producer) = self.producer(service, session, native).await? else {
                continue;
            };
            if matches!(self.recovered.get(session), Some(Recovery::Ready { turn_id, .. }) if turn_id == native)
                && self
                    .output
                    .snapshot(session, &producer.identity.turn_id)
                    .is_some()
            {
                continue;
            }
            // Missing/summary items cannot establish a trustworthy text baseline.
            if matches!(turn["itemsView"].as_str(), Some(view) if view != "full") {
                self.unavailable(session, native);
                continue;
            }
            let Some(items) = turn["items"].as_array() else {
                self.unavailable(session, native);
                continue;
            };
            // Legacy full-history snapshots synthesize item-N identities, whereas live
            // notifications use the native message ID. Those snapshots cannot safely
            // cover queued deltas. Fail closed rather than infer item correspondence.
            if items.iter().any(|item| {
                item["type"] == "agentMessage" && is_history_item_id(item["id"].as_str())
            }) {
                self.unavailable(session, native);
                continue;
            }
            let items = items
                .iter()
                .filter(|item| item["type"] == "agentMessage")
                .map(|item| {
                    Ok(LiveOutputItem::AssistantText {
                        item_id: string(item, "id")?.into(),
                        text: item["text"]
                            .as_str()
                            .ok_or_else(|| {
                                pontia_core::Error::Domain(
                                    "Codex agent message missing text".into(),
                                )
                            })?
                            .into(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let sequence = self
                .output
                .snapshot(session, &producer.identity.turn_id)
                .map_or(1, |snapshot| snapshot.sequence + 1);
            let covered_items = items
                .iter()
                .filter_map(|item| match item {
                    LiveOutputItem::AssistantText { item_id, .. } => Some(item_id.clone()),
                    _ => None,
                })
                .collect();
            self.output
                .replace_snapshot(LiveOutputSnapshotReplacement {
                    producer,
                    sequence,
                    items,
                })
                .await?;
            self.recovered.insert(
                session.into(),
                Recovery::Ready {
                    turn_id: native.into(),
                    boundary: snapshot.notification_boundary,
                    items: covered_items,
                },
            );
        }
        Ok(())
    }

    pub async fn delta(
        &mut self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        notification: &Notification,
    ) -> Result<()> {
        let params = &notification["params"];
        let thread = string(params, "threadId")?;
        let native = string(params, "turnId")?;
        let item_id = string(params, "itemId")?;
        let delta = params["delta"]
            .as_str()
            .ok_or_else(|| pontia_core::Error::Domain("Codex delta must be text".into()))?;
        if delta.is_empty() {
            return Ok(());
        }
        if matches!(self.recovered.get(session), Some(Recovery::Unavailable { turn_id }) if turn_id == native)
        {
            return Ok(());
        }
        let _operation = runtime.lock_session(session).await;
        for attempt in 0..2 {
            let turn_id = {
                let _current = runtime.current_guard().await?;
                if !self.accepts(service, runtime, session, thread).await? {
                    return Ok(());
                }
                let Some(producer) = self.producer(service, session, native).await? else {
                    return Ok(());
                };
                producer.identity.turn_id
            };
            if attempt > 0 || self.output.snapshot(session, &turn_id).is_none() {
                self.recovered.remove(session);
                self.restore(service, runtime, session, thread).await?;
            }
            let _current = runtime.current_guard().await?;
            if !self.accepts(service, runtime, session, thread).await? {
                return Ok(());
            }
            let Some(producer) = self.producer(service, session, native).await? else {
                return Ok(());
            };
            // Full native items cover queued notifications for those items only.
            // Unfinished items may be absent from native history; retain their deltas.
            if matches!(self.recovered.get(session), Some(Recovery::Ready { turn_id, boundary, items })
                if turn_id == native && notification.sequence <= *boundary && items.contains(item_id))
            {
                return Ok(());
            }
            let Some(snapshot) = self.output.snapshot(session, &turn_id) else {
                return Ok(());
            };
            let outcome = self
                .output
                .publish_batch(LiveOutputBatch {
                    producer,
                    first_sequence: snapshot.sequence + 1,
                    updates: vec![LiveOutputUpdate::AssistantTextDelta {
                        item_id: item_id.into(),
                        delta: delta.into(),
                    }],
                })
                .await?;
            if matches!(outcome, LiveOutputPublishOutcome::Accepted { .. }) {
                return Ok(());
            }
        }
        Ok(())
    }

    async fn accepts(
        &self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        thread: &str,
    ) -> Result<bool> {
        if runtime.subscription(session).await != Some(SubscriptionState::Available) {
            return Ok(false);
        }
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
                .bind(session)
                .fetch_optional(&service.pool)
                .await?;
        if !matches!(state.as_deref(), Some("idle" | "busy")) {
            return Ok(false);
        }
        Ok(AgentBindingService::new(service.pool.clone())
            .binding_for_session(session)
            .await?
            .is_some_and(|binding| {
                binding.client_type == "codex" && binding.client_session_key == thread
            }))
    }

    async fn producer(
        &self,
        service: &CodexService,
        session: &str,
        native: &str,
    ) -> Result<Option<LiveOutputProducer>> {
        let turn: Option<String> = sqlx::query_scalar("SELECT t.turn_id FROM native_turn_bindings b JOIN turns t ON t.turn_id=b.turn_id WHERE b.session_id=? AND b.client_turn_id=? AND t.session_id=? AND t.state='running'")
            .bind(session).bind(native).bind(session).fetch_optional(&service.pool).await?;
        Ok(turn.map(|turn_id| LiveOutputProducer {
            identity: LiveOutputIdentity {
                session_id: session.into(),
                stream_id: turn_id.clone(),
                turn_id,
            },
            source: LiveOutputSource::SharedBackend,
        }))
    }
}

fn is_history_item_id(id: Option<&str>) -> bool {
    id.and_then(|id| id.strip_prefix("item-"))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}
