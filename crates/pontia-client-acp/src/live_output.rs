//! ACP presentation conversion. The shared store is the only retained content cache;
//! prompts retain only bounded result-provenance IDs, never raw tool payloads.
use pontia_application::{
    LiveOutputBatch, LiveOutputIdentity, LiveOutputItem, LiveOutputProducer,
    LiveOutputPublishOutcome, LiveOutputService, LiveOutputSnapshotReplacement, LiveOutputSource,
    LiveOutputUpdate, LiveReasoningBound, LiveToolOutcome, LiveToolPhase, LiveToolResult,
    LiveToolStatus,
};
use pontia_core::{Error, Result};
use serde_json::Value;

use crate::peer::Prompt;

pub(crate) async fn publish(
    output: &LiveOutputService,
    session: &str,
    runtime: &str,
    prompt: &mut Prompt,
    update: &Value,
) -> Result<()> {
    let producer = LiveOutputProducer {
        identity: LiveOutputIdentity {
            session_id: session.into(),
            turn_id: prompt.turn.clone(),
            stream_id: format!("{runtime}:{}", prompt.turn),
        },
        source: LiveOutputSource::RuntimeBound {
            runtime_id: runtime.into(),
        },
    };
    let items = match output.snapshot(session, &prompt.turn) {
        Some(snapshot)
            if snapshot.identity == producer.identity && snapshot.sequence == prompt.sequence =>
        {
            snapshot.items
        }
        None if prompt.sequence == 0 => Vec::new(),
        _ => return Err(invalidated()),
    };
    let mut publisher = Publisher {
        output,
        producer,
        prompt,
        items,
    };
    if matches!(
        update["sessionUpdate"].as_str(),
        Some("agent_message_chunk" | "tool_call" | "tool_call_update")
    ) {
        publisher.prompt.thought = None;
    }
    match update["sessionUpdate"].as_str() {
        Some("agent_message_chunk")
            if update.pointer("/content/type").and_then(Value::as_str) == Some("text") =>
        {
            if let Some(text) = update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                if publisher.prompt.sequence == 0 {
                    publisher
                        .snapshot(vec![LiveOutputItem::AssistantText {
                            item_id: "response".into(),
                            text: text.into(),
                        }])
                        .await?;
                } else {
                    publisher
                        .send(LiveOutputUpdate::AssistantTextDelta {
                            item_id: "response".into(),
                            delta: text.into(),
                        })
                        .await?;
                }
            }
        }
        Some("agent_thought_chunk")
            if update.pointer("/content/type").and_then(Value::as_str) == Some("text") =>
        {
            if let Some(text) = update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                publisher.thought(text).await?;
            }
        }
        Some("tool_call" | "tool_call_update") => publisher.tool(update).await?,
        _ => {}
    }
    Ok(())
}

struct Publisher<'a> {
    output: &'a LiveOutputService,
    producer: LiveOutputProducer,
    prompt: &'a mut Prompt,
    items: Vec<LiveOutputItem>,
}

impl Publisher<'_> {
    /// Consecutive thought chunks extend one item. Assistant text always extends
    /// the single response item, so adjacency is tracked per update, not per item.
    async fn thought(&mut self, text: &str) -> Result<()> {
        let item_id = self
            .prompt
            .thought
            .get_or_insert_with(|| format!("thought-{}", self.items.len()))
            .clone();
        let update = LiveOutputUpdate::ReasoningDelta {
            item_id,
            delta: text.into(),
            truncated: false,
        };
        match update.bound_reasoning(&self.items) {
            LiveReasoningBound::Update(LiveOutputUpdate::ReasoningDelta {
                item_id,
                delta,
                truncated,
            }) if self.prompt.sequence == 0 => {
                self.snapshot(vec![LiveOutputItem::Reasoning {
                    item_id,
                    text: delta,
                    truncated,
                }])
                .await
            }
            LiveReasoningBound::Update(update) => self.send(update).await,
            LiveReasoningBound::Snapshot(items) => self.snapshot(items).await,
            LiveReasoningBound::Drop => Ok(()),
        }
    }

    async fn tool(&mut self, update: &Value) -> Result<()> {
        let Some(call_id) = update["toolCallId"].as_str() else {
            return Ok(());
        };
        // `kind` is a category, not a concrete tool protocol. In particular it
        // must never opt a call into managed bash/read/edit/write rendering.
        let name = update["name"].as_str().or_else(|| update["title"].as_str());
        let existing = self.items.iter().position(|item| {
            matches!(item,
            LiveOutputItem::ToolCall { call_id: id, .. } if id == call_id)
        });
        if let Some(index) = existing {
            let mut items = self.items.clone();
            let LiveOutputItem::ToolCall {
                tool_name,
                arguments,
                ..
            } = &mut items[index]
            else {
                unreachable!()
            };
            if let Some(name) = name {
                *tool_name = name.into();
            }
            if let Some(input) = update.get("rawInput") {
                *arguments = input.clone();
            }
            if items != self.items {
                // There is deliberately no public call-update event. Preserve
                // all other entries and confirmed execution facts in the snapshot.
                self.snapshot(items).await?;
            }
        } else {
            let Some(name) = name else { return Ok(()) };
            let arguments = update.get("rawInput").cloned().unwrap_or(Value::Null);
            // Local item IDs cannot collide with arbitrary native call IDs or text.
            let item_id = format!("tool-{}", self.items.len());
            if self.prompt.sequence == 0 {
                self.snapshot(vec![LiveOutputItem::ToolCall {
                    item_id,
                    call_id: call_id.into(),
                    tool_name: name.into(),
                    arguments,
                    managed_tool_use: None,
                    status: None,
                    output: None,
                    result: None,
                }])
                .await?;
            } else {
                self.send(LiveOutputUpdate::ToolCall {
                    item_id,
                    call_id: call_id.into(),
                    tool_name: name.into(),
                    arguments,
                    managed_tool_use: None,
                })
                .await?;
            }
        }

        let status = match update["status"].as_str() {
            Some("in_progress") => Some(LiveToolStatus {
                phase: LiveToolPhase::Running,
                outcome: None,
            }),
            Some("completed") => Some(LiveToolStatus {
                phase: LiveToolPhase::Ended,
                outcome: Some(LiveToolOutcome::Succeeded),
            }),
            Some("failed") => Some(LiveToolStatus {
                phase: LiveToolPhase::Ended,
                outcome: Some(LiveToolOutcome::Failed),
            }),
            _ => None,
        };
        if let Some(status) = status {
            self.send(LiveOutputUpdate::ToolStatusUpdated {
                call_id: call_id.into(),
                status,
            })
            .await?;
        }
        let text = update
            .get("content")
            .and_then(Value::as_array)
            .map(|content| {
                content
                    .iter()
                    .filter_map(|item| {
                        if item["type"] != "content" {
                            return None;
                        }
                        match item.pointer("/content/type").and_then(Value::as_str) {
                            Some("text") => item.pointer("/content/text").and_then(Value::as_str),
                            Some("resource") => item
                                .pointer("/content/resource/text")
                                .and_then(Value::as_str),
                            _ => None,
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        // Unsupported content (e.g. terminal references) is not an empty text
        // output. It only clears text when replacing a previously visible output.
        if let Some(text) = &text
            && (!text.is_empty()
                || self.items.iter().any(|item| {
                    matches!(item,
                LiveOutputItem::ToolCall { call_id: id, output: Some(_), .. } if id == call_id)
                }))
        {
            self.send(LiveOutputUpdate::ToolOutputUpdated {
                call_id: call_id.into(),
                text: text.clone(),
                truncated: false,
            })
            .await?;
        }
        // rawOutput is the native return value, independently of status. Text
        // content is only a result when the same notification confirms completion.
        // A status-only completion never manufactures a result from cached output.
        let result = update.get("rawOutput").cloned().or_else(|| {
            (matches!(update["status"].as_str(), Some("completed" | "failed"))
                && !self.prompt.raw_results.contains(call_id))
            .then(|| text.filter(|text| !text.is_empty()).map(Value::String))
            .flatten()
        });
        if let Some(value) = result {
            let failed = self.items.iter().any(|item| matches!(item,
                LiveOutputItem::ToolCall { call_id: id, status: Some(LiveToolStatus { outcome: Some(LiveToolOutcome::Failed), .. }), .. } if id == call_id));
            self.send(LiveOutputUpdate::ToolResult {
                call_id: call_id.into(),
                result: LiveToolResult {
                    content: (!failed).then(|| value.clone()),
                    error: failed.then_some(value),
                    truncated: false,
                },
            })
            .await?;
            if update.get("rawOutput").is_some() {
                self.prompt.raw_results.insert(call_id.into());
            }
        }
        Ok(())
    }

    async fn snapshot(&mut self, items: Vec<LiveOutputItem>) -> Result<()> {
        let outcome = self
            .output
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: self.producer.clone(),
                sequence: self.prompt.sequence + 1,
                items,
            })
            .await?;
        self.accept(outcome)
    }

    async fn send(&mut self, update: LiveOutputUpdate) -> Result<()> {
        let update = update.bound_tool_content(&self.items);
        let outcome = self
            .output
            .publish_batch(LiveOutputBatch {
                producer: self.producer.clone(),
                first_sequence: self.prompt.sequence + 1,
                updates: vec![update],
            })
            .await?;
        self.accept(outcome)
    }

    fn accept(&mut self, outcome: LiveOutputPublishOutcome) -> Result<()> {
        match outcome {
            LiveOutputPublishOutcome::Accepted {
                accepted_sequence, ..
            } => {
                self.prompt.sequence = accepted_sequence;
                self.items = self
                    .output
                    .snapshot(&self.producer.identity.session_id, &self.prompt.turn)
                    .ok_or_else(invalidated)?
                    .items;
                Ok(())
            }
            LiveOutputPublishOutcome::SnapshotRequired { .. } => Err(invalidated()),
        }
    }
}

fn invalidated() -> Error {
    Error::CapabilityUnavailable("ACP live output stream was invalidated".into())
}
