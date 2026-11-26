//! ACP presentation conversion. The shared store is the only retained content cache;
//! prompts retain only bounded result-provenance IDs, never raw tool payloads.
use pontia_agent_control::{
    LiveOutputBatch, LiveOutputIdentity, LiveOutputItem, LiveOutputProducer,
    LiveOutputPublishOutcome, LiveOutputService, LiveOutputSnapshotReplacement, LiveOutputSource,
    LiveOutputUpdate, LiveReasoningBound, LiveToolOutcome, LiveToolPhase, LiveToolResult,
    LiveToolStatus,
    client_contract::raw_transcripts::{
        ManagedFileChange, ManagedFileOperation, ManagedPlanEntry, ManagedToolUse,
        ManagedToolUseInput,
    },
    live_output::{MAX_ITEMS_PER_STREAM, apply_update},
};
use pontia_core::{Error, Result};
use serde_json::Value;

use crate::peer::Prompt;

/// Places steering the agent accepted into the running prompt at its position.
pub(crate) async fn user_input(
    output: &LiveOutputService,
    session: &str,
    runtime: &str,
    prompt: &mut Prompt,
    text: &str,
) -> Result<()> {
    let mut publisher = publisher(output, session, runtime, prompt)?;
    publisher.prompt.thought = None;
    let item_id = format!("input-{}", publisher.prompt.sequence);
    publisher
        .send(LiveOutputUpdate::UserInput {
            item_id,
            text: text.into(),
        })
        .await
}

pub(crate) async fn publish(
    output: &LiveOutputService,
    session: &str,
    runtime: &str,
    prompt: &mut Prompt,
    update: &Value,
) -> Result<()> {
    let mut publisher = publisher(output, session, runtime, prompt)?;
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
                let item_id = publisher
                    .items
                    .iter()
                    .rev()
                    .find_map(|item| match item {
                        LiveOutputItem::AssistantText { item_id, .. } => Some(item_id.clone()),
                        LiveOutputItem::UserInput { item_id, .. } => {
                            Some(format!("response-{item_id}"))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| format!("response-{}", publisher.prompt.sequence));
                publisher
                    .send(LiveOutputUpdate::AssistantTextDelta {
                        item_id,
                        delta: text.into(),
                    })
                    .await?;
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
        Some("plan") => publisher.plan(update).await?,
        _ => {}
    }
    Ok(())
}

pub(crate) fn tool_text(update: &Value) -> Option<String> {
    update
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
        })
}

fn publisher<'a>(
    output: &'a LiveOutputService,
    session: &str,
    runtime: &str,
    prompt: &'a mut Prompt,
) -> Result<Publisher<'a>> {
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
    Ok(Publisher {
        output,
        producer,
        prompt,
        items,
    })
}

struct Publisher<'a> {
    output: &'a LiveOutputService,
    producer: LiveOutputProducer,
    prompt: &'a mut Prompt,
    items: Vec<LiveOutputItem>,
}

impl Publisher<'_> {
    /// Consecutive thought chunks extend one item until other output arrives.
    async fn thought(&mut self, text: &str) -> Result<()> {
        let item_id = self
            .prompt
            .thought
            .get_or_insert_with(|| format!("thought-{}", self.prompt.sequence))
            .clone();
        let update = LiveOutputUpdate::ReasoningDelta {
            item_id,
            delta: text.into(),
            truncated: false,
        };
        match update.bound_reasoning(&self.items) {
            LiveReasoningBound::Update(update) => self.send(update).await,
            LiveReasoningBound::Snapshot(items) => self.snapshot(items).await,
            LiveReasoningBound::Drop => Ok(()),
        }
    }

    async fn tool(&mut self, update: &Value) -> Result<()> {
        let Some(call_id) = update["toolCallId"].as_str() else {
            return Ok(());
        };
        self.move_plan_from_call_id(call_id).await?;
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
                managed_tool_use,
                ..
            } = &mut items[index]
            else {
                unreachable!()
            };
            if let Some(name) = name {
                *tool_name = name.into();
                if let Some(managed) = managed_tool_use {
                    managed.tool_name = name.into();
                }
            }
            if let Some(input) = update.get("rawInput") {
                *arguments = input.clone();
            }
            if update.get("content").is_some() {
                *managed_tool_use = managed_tool_use_from_update(tool_name, update).map(Box::new);
            }
            if items != self.items {
                // There is deliberately no public call-update event. Preserve
                // all other entries and confirmed execution facts in the snapshot.
                self.snapshot(items).await?;
            }
        } else {
            let Some(name) = name else { return Ok(()) };
            let arguments = update.get("rawInput").cloned().unwrap_or(Value::Null);
            let managed_tool_use = managed_tool_use_from_update(name, update);
            // Local item IDs cannot collide with arbitrary native call IDs or text.
            let item_id = format!("tool-{}", self.prompt.sequence);
            self.send(LiveOutputUpdate::ToolCall {
                item_id,
                call_id: call_id.into(),
                tool_name: name.into(),
                arguments,
                managed_tool_use,
            })
            .await?;
            if !self.items.iter().any(|item| matches!(item, LiveOutputItem::ToolCall { call_id: id, .. } if id == call_id)) {
                return Ok(());
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
        let text = tool_text(update);
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

    async fn plan(&mut self, update: &Value) -> Result<()> {
        let arguments = serde_json::json!({"entries": update["entries"]});
        let managed_tool_use = plan_tool_use(update);
        if let Some(index) = self.items.iter().position(is_plan_item) {
            let mut items = self.items.clone();
            let LiveOutputItem::ToolCall {
                arguments: current_arguments,
                managed_tool_use: current_tool_use,
                ..
            } = &mut items[index]
            else {
                unreachable!()
            };
            *current_arguments = arguments;
            *current_tool_use = Some(Box::new(managed_tool_use));
            return self.snapshot(items).await;
        }
        self.send(LiveOutputUpdate::ToolCall {
            item_id: format!("plan-{}", self.prompt.sequence),
            call_id: available_plan_call_id(&self.items, None, None),
            tool_name: PLAN_TOOL_NAME.into(),
            arguments,
            managed_tool_use: Some(managed_tool_use),
        })
        .await
    }

    async fn move_plan_from_call_id(&mut self, native_call_id: &str) -> Result<()> {
        let Some(index) = self.items.iter().position(|item| {
            matches!(item, LiveOutputItem::ToolCall { call_id, .. } if call_id == native_call_id)
                && is_plan_item(item)
        }) else {
            return Ok(());
        };
        let mut items = self.items.clone();
        let replacement = available_plan_call_id(&items, Some(index), Some(native_call_id));
        let LiveOutputItem::ToolCall { call_id, .. } = &mut items[index] else {
            unreachable!()
        };
        *call_id = replacement;
        self.snapshot(items).await
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
        let new_item = match &update {
            LiveOutputUpdate::AssistantTextDelta { item_id, .. }
            | LiveOutputUpdate::ReasoningDelta { item_id, .. } => {
                !self.items.iter().any(|item| item.item_id() == item_id)
            }
            LiveOutputUpdate::ToolCall { .. } | LiveOutputUpdate::UserInput { .. } => true,
            _ => false,
        };
        if self.prompt.sequence == 0 || (new_item && self.items.len() == MAX_ITEMS_PER_STREAM) {
            let mut items = std::mem::take(&mut self.items);
            let evicted = if items.len() == MAX_ITEMS_PER_STREAM {
                let evict = items.iter().position(|item| match item {
                    LiveOutputItem::ToolCall { status, .. } => status
                        .as_ref()
                        .is_some_and(|s| s.phase == LiveToolPhase::Ended),
                    _ => true,
                });
                let Some(index) = evict else {
                    // Every visible call is still in flight. Keep accepting their updates.
                    return self.snapshot(items).await;
                };
                Some(items.remove(index))
            } else {
                None
            };
            apply_update(&mut items, update)?;
            self.snapshot(items).await?;
            if let Some(LiveOutputItem::ToolCall { call_id, .. }) = evicted {
                self.prompt.raw_results.remove(&call_id);
            }
            return Ok(());
        }
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

pub(crate) fn managed_tool_use_from_update(name: &str, update: &Value) -> Option<ManagedToolUse> {
    let changes = update["content"]
        .as_array()?
        .iter()
        .filter(|item| item["type"] == "diff")
        .map(|item| {
            let path = item["path"].as_str()?;
            let old_text = item.get("oldText").and_then(Value::as_str);
            let new_text = item["newText"].as_str()?;
            Some(ManagedFileChange {
                path: path.into(),
                operation: if old_text.is_some() {
                    ManagedFileOperation::Update
                } else {
                    ManagedFileOperation::Add
                },
                move_path: None,
                diff: whole_file_diff(path, old_text, new_text),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!changes.is_empty()).then(|| ManagedToolUse {
        tool_name: name.into(),
        input: ManagedToolUseInput::FileChanges { changes },
    })
}

fn whole_file_diff(path: &str, old_text: Option<&str>, new_text: &str) -> String {
    let old_lines = old_text.map_or(0, |text| text.lines().count());
    let new_lines = new_text.lines().count();
    let mut diff = format!("--- {path}\n+++ {path}\n@@ -1,{old_lines} +1,{new_lines} @@\n");
    if let Some(old_text) = old_text {
        append_diff_text(&mut diff, '-', old_text);
    }
    append_diff_text(&mut diff, '+', new_text);
    diff
}

fn append_diff_text(diff: &mut String, prefix: char, text: &str) {
    for line in text.split_inclusive('\n') {
        diff.push(prefix);
        diff.push_str(line);
        if !line.ends_with('\n') {
            diff.push('\n');
        }
    }
    if !text.is_empty() && !text.ends_with('\n') {
        diff.push_str("\\ No newline at end of file\n");
    }
}

pub(crate) const PLAN_TOOL_NAME: &str = "plan";

fn is_plan_item(item: &LiveOutputItem) -> bool {
    matches!(item, LiveOutputItem::ToolCall { managed_tool_use: Some(tool), .. }
        if matches!(tool.input, ManagedToolUseInput::Plan { .. }))
}

fn available_plan_call_id(
    items: &[LiveOutputItem],
    excluded: Option<usize>,
    reserved: Option<&str>,
) -> String {
    for ordinal in 0_u64.. {
        let candidate = if ordinal == 0 {
            "acp-plan".into()
        } else {
            format!("acp-plan-{ordinal}")
        };
        if reserved != Some(candidate.as_str()) && !items.iter().enumerate().any(|(index, item)| {
            Some(index) != excluded
                && matches!(item, LiveOutputItem::ToolCall { call_id, .. } if call_id == &candidate)
        }) {
            return candidate;
        }
    }
    unreachable!()
}

pub(crate) fn plan_tool_use(update: &Value) -> ManagedToolUse {
    let entries = update["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(ManagedPlanEntry {
                content: entry["content"].as_str()?.into(),
                priority: entry["priority"].as_str()?.into(),
                status: entry["status"].as_str()?.into(),
            })
        })
        .collect();
    ManagedToolUse {
        tool_name: PLAN_TOOL_NAME.into(),
        input: ManagedToolUseInput::Plan { entries },
    }
}

fn invalidated() -> Error {
    Error::CapabilityUnavailable("ACP live output stream was invalidated".into())
}
