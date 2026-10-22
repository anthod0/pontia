use super::{CodexService, string};
use crate::runtime::{CodexRuntime, SubscriptionState, protocol::Notification};
use pontia_application::{
    AgentBindingService, LiveOutputBatch, LiveOutputIdentity, LiveOutputItem, LiveOutputProducer,
    LiveOutputPublishOutcome, LiveOutputService, LiveOutputSnapshotReplacement, LiveOutputSource,
    LiveOutputUpdate,
};
use pontia_core::Result;
use std::collections::{HashMap, HashSet};

mod tool_calls;

enum TextRecovery {
    Ready {
        turn_id: String,
        boundary: u64,
        items: HashSet<String>,
    },
    Unavailable {
        turn_id: String,
    },
}

struct ToolRecovery {
    turn_id: String,
    boundary: u64,
    outputs: HashSet<String>,
    results: HashSet<String>,
}

pub(super) struct CodexLiveOutput {
    output: LiveOutputService,
    recovered: HashMap<String, TextRecovery>,
    tool_baselines: HashMap<String, ToolRecovery>,
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
            tool_baselines: HashMap::new(),
        }
    }

    pub fn invalidate(&mut self, session: &str) {
        self.recovered.remove(session);
        self.tool_baselines.remove(session);
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
            TextRecovery::Unavailable {
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
            if matches!(self.recovered.get(session), Some(TextRecovery::Ready { turn_id, .. } | TextRecovery::Unavailable { turn_id }) if turn_id == native)
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
            // Legacy text IDs cannot cover queued deltas. Keep text fail-closed,
            // but recover tools whose native call IDs remain stable in that history.
            let unsafe_text = items.iter().any(|item| {
                item["type"] == "agentMessage" && is_history_item_id(item["id"].as_str())
            });
            let mut recovered_items = Vec::new();
            let mut covered_tools = HashSet::new();
            let mut covered_results = HashSet::new();
            for item in items {
                if item["type"] == "agentMessage" && !unsafe_text {
                    recovered_items.push(LiveOutputItem::AssistantText {
                        item_id: string(item, "id")?.into(),
                        text: item["text"]
                            .as_str()
                            .ok_or_else(|| {
                                pontia_core::Error::Domain(
                                    "Codex agent message missing text".into(),
                                )
                            })?
                            .into(),
                    });
                } else if let Some(call) = tool_calls::from_native(item) {
                    recovered_items.push(call.into_item());
                }
            }
            // Reserve all invocation inputs before any output consumes the budget.
            for item in items {
                if let Some(index) = recovered_items.iter().position(|call| matches!(call, LiveOutputItem::ToolCall { call_id, .. } if item["id"].as_str() == Some(call_id))) {
                    for update in tool_calls::execution(item, None) {
                        let update = update.bound_tool_content(&recovered_items);
                        if let Some(LiveOutputItem::ToolCall { status, result, .. }) =
                            recovered_items.get_mut(index)
                        {
                            match update {
                                LiveOutputUpdate::ToolStatusUpdated { status: value, .. } => {
                                    *status = Some(value)
                                }
                                LiveOutputUpdate::ToolResult {
                                    call_id,
                                    result: value,
                                } => {
                                    covered_results.insert(call_id);
                                    *result = Some(Box::new(value))
                                }
                                _ => {}
                            }
                        }
                    }
                    if item["type"] == "commandExecution"
                        && let Some(text) = item["aggregatedOutput"].as_str()
                    {
                        // Native history is a bounded observation, not proof that
                        // every byte since execution started is available.
                        let update = LiveOutputUpdate::ToolOutputUpdated {
                            call_id: item["id"].as_str().unwrap().into(),
                            text: text.into(),
                            truncated: true,
                        }
                        .bound_tool_content(&recovered_items);
                        if let LiveOutputUpdate::ToolOutputUpdated {
                            call_id,
                            text,
                            truncated,
                        } = update
                        {
                            covered_tools.insert(call_id);
                            if let Some(LiveOutputItem::ToolCall { output, .. }) =
                                recovered_items.get_mut(index)
                            {
                                *output =
                                    Some(pontia_application::LiveToolOutput { text, truncated });
                            }
                        }
                    }
                }
            }
            let items = recovered_items;
            if unsafe_text && items.is_empty() {
                self.unavailable(session, native);
                continue;
            }
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
            self.tool_baselines.insert(
                session.into(),
                ToolRecovery {
                    turn_id: native.into(),
                    boundary: snapshot.notification_boundary,
                    outputs: covered_tools,
                    results: covered_results,
                },
            );
            self.recovered.insert(
                session.into(),
                if unsafe_text {
                    TextRecovery::Unavailable {
                        turn_id: native.into(),
                    }
                } else {
                    TextRecovery::Ready {
                        turn_id: native.into(),
                        boundary: snapshot.notification_boundary,
                        items: covered_items,
                    }
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
        let item_id = string(params, "itemId")?;
        let delta = params["delta"]
            .as_str()
            .ok_or_else(|| pontia_core::Error::Domain("Codex delta must be text".into()))?;
        if delta.is_empty() {
            return Ok(());
        }
        self.publish(
            service,
            runtime,
            session,
            notification,
            LiveOutputUpdate::AssistantTextDelta {
                item_id: item_id.into(),
                delta: delta.into(),
            },
        )
        .await
    }

    pub async fn tool_call(
        &mut self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        notification: &Notification,
    ) -> Result<()> {
        let item = &notification["params"]["item"];
        if let Some(call) = tool_calls::from_native(item) {
            self.publish(service, runtime, session, notification, call.into_update())
                .await?;
        }
        for update in tool_calls::execution(item, notification["method"].as_str()) {
            self.publish(service, runtime, session, notification, update)
                .await?;
        }
        Ok(())
    }

    pub async fn tool_output(
        &mut self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        notification: &Notification,
    ) -> Result<()> {
        let params = &notification["params"];
        let call_id = string(params, "itemId")?;
        let delta = string(params, "delta")?;
        if delta.is_empty() {
            return Ok(());
        }
        self.publish(
            service,
            runtime,
            session,
            notification,
            LiveOutputUpdate::ToolOutputDelta {
                call_id: call_id.into(),
                delta: delta.into(),
                truncated: false,
            },
        )
        .await
    }

    async fn publish(
        &mut self,
        service: &CodexService,
        runtime: &CodexRuntime,
        session: &str,
        notification: &Notification,
        update: LiveOutputUpdate,
    ) -> Result<()> {
        let params = &notification["params"];
        let thread = string(params, "threadId")?;
        let native = string(params, "turnId")?;
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
            if matches!(&update, LiveOutputUpdate::AssistantTextDelta { .. })
                && matches!(self.recovered.get(session), Some(TextRecovery::Unavailable { turn_id }) if turn_id == native)
            {
                return Ok(());
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
            if matches!(&update, LiveOutputUpdate::AssistantTextDelta { item_id, .. }
                if matches!(self.recovered.get(session), Some(TextRecovery::Ready { turn_id, boundary, items })
                    if turn_id == native && notification.sequence <= *boundary && items.contains(item_id)))
            {
                return Ok(());
            }
            // A valid invocation does not need a safe assistant-text baseline.
            // Native history can omit an unfinished tool even when text recovery
            // is unavailable; seed a tool-only stream from its notification.
            if matches!(&update, LiveOutputUpdate::ToolCall { .. })
                && self.output.snapshot(session, &turn_id).is_none()
                && matches!(self.recovered.get(session), Some(TextRecovery::Unavailable { turn_id }) if turn_id == native)
            {
                self.output
                    .replace_snapshot(LiveOutputSnapshotReplacement {
                        producer: producer.clone(),
                        sequence: 1,
                        items: Vec::new(),
                    })
                    .await?;
            }
            let Some(snapshot) = self.output.snapshot(session, &turn_id) else {
                return Ok(());
            };
            // ToolCall adds an immutable call, never updates its result or status.
            // The same native ID is used by snapshots, starts, completions and replays.
            if let LiveOutputUpdate::ToolCall { call_id, .. } = &update
                && snapshot.items.iter().any(|item| {
                    matches!(item,
                    LiveOutputItem::ToolCall { call_id: existing, .. } if existing == call_id)
                })
            {
                return Ok(());
            }
            // Execution content never creates an anonymous tool. Recovery may
            // discover the call, otherwise ignore uncorrelatable notifications.
            let call_id = match &update {
                LiveOutputUpdate::ToolStatusUpdated { call_id, .. }
                | LiveOutputUpdate::ToolResult { call_id, .. }
                | LiveOutputUpdate::ToolOutputDelta { call_id, .. }
                | LiveOutputUpdate::ToolOutputUpdated { call_id, .. } => Some(call_id),
                _ => None,
            };
            if let Some(call_id) = call_id {
                let Some(LiveOutputItem::ToolCall { status, result, output, .. }) = snapshot.items.iter().find(|item| matches!(item, LiveOutputItem::ToolCall { call_id: id, .. } if id == call_id)) else { return Ok(()); };
                // A repeated start's initial aggregate must not replace newer deltas.
                if matches!(&update, LiveOutputUpdate::ToolOutputUpdated { .. }) && output.is_some()
                {
                    return Ok(());
                }
                if let LiveOutputUpdate::ToolStatusUpdated {
                    status: incoming, ..
                } = &update
                    && let Some(current) = status
                {
                    let mut merged = current.clone();
                    merged.merge(incoming.clone());
                    if &merged == current {
                        return Ok(());
                    }
                }
                if matches!(&update, LiveOutputUpdate::ToolResult { .. })
                    && self.tool_baselines.get(session).is_some_and(|baseline| {
                        baseline.turn_id == native
                            && notification.sequence <= baseline.boundary
                            && baseline.results.contains(call_id)
                    })
                {
                    return Ok(());
                }
                if matches!(&update, LiveOutputUpdate::ToolResult { result: incoming, .. } if result.as_deref() == Some(incoming))
                {
                    return Ok(());
                }
                if matches!(&update, LiveOutputUpdate::ToolOutputDelta { .. })
                    && self.tool_baselines.get(session).is_some_and(|baseline| {
                        baseline.turn_id == native
                            && notification.sequence <= baseline.boundary
                            && baseline.outputs.contains(call_id)
                    })
                {
                    return Ok(());
                }
            }
            let bounded = update.clone().bound_tool_content(&snapshot.items);
            let outcome = self
                .output
                .publish_batch(LiveOutputBatch {
                    producer,
                    first_sequence: snapshot.sequence + 1,
                    updates: vec![bounded],
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
        if !matches!(
            state.as_deref(),
            Some("idle" | "busy" | "waiting_for_interaction")
        ) {
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
