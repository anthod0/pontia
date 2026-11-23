use super::*;
use std::collections::{HashMap, VecDeque};
use tokio::sync::{Notify, oneshot};

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Observation {
    Setup {
        binding: String,
        metadata: Value,
        model: Option<String>,
        cwd: String,
    },
    Started {
        turn: String,
        anchor: Value,
    },
    Finished {
        turn: String,
        anchor: Value,
        reason: String,
    },
    PromotionPending {
        native: String,
    },
    PromotionStarted {
        native: String,
        digest: String,
        summary: String,
        index: usize,
    },
    PromotionFinished {
        native: String,
        reason: String,
        index: usize,
    },
    AutoCompactionStarted {
        operation: String,
    },
    UsageEstimate {
        ratio: f64,
        size: Option<u64>,
    },
    Compacted {
        operation: String,
        manual: bool,
        tokens_before: Option<u64>,
        tokens_after: Option<u64>,
    },
    CompactionFailed {
        operation: String,
        manual: bool,
        aborted: bool,
        message: String,
    },
    Usage {
        used: u64,
        size: u64,
    },
    Model {
        model: String,
    },
    Permission {
        frame: Value,
    },
    Delivered {
        registration: String,
        digest: String,
    },
    Cancelled {
        registration: String,
        digest: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Report {
    pub key: String,
    pub observation: Observation,
}

pub(super) struct PendingReport {
    report: Report,
    complete: watch::Sender<()>,
    confirmed: Option<oneshot::Sender<Result<()>>>,
}

struct Promotion {
    id: String,
    digest: String,
    summary: String,
    index: usize,
    started: bool,
}

#[derive(Default)]
pub(super) struct Reports {
    pending: VecDeque<PendingReport>,
    // Original requests are retained only while their native callbacks are waiting.
    permissions: HashMap<String, Report>,
    writing: std::collections::HashSet<String>,
    pub wake: Arc<Notify>,
    started: Option<String>,
    promotion: Option<Promotion>,
    automatic_compaction: Option<String>,
    next_index: usize,
    original_terminal: Option<(String, String)>,
    original_native: Option<String>,
    finished: Option<String>,
    pub(super) close_request_id: Option<Value>,
    closed: bool,
}

impl Reports {
    pub fn barrier(&self) -> Option<watch::Receiver<()>> {
        self.pending
            .back()
            .map(|report| report.complete.subscribe())
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn push(&mut self, observation: Observation) -> oneshot::Receiver<Result<()>> {
        let report = Report {
            key: format!("acp:frame:{}", pontia_core::ids::new_event_id()),
            observation,
        };
        if let Observation::Permission { frame } = &report.observation {
            self.permissions
                .insert(frame["id"].to_string(), report.clone());
        }
        let (send, recv) = oneshot::channel();
        self.pending.push_back(PendingReport {
            report,
            complete: watch::channel(()).0,
            confirmed: Some(send),
        });
        self.wake.notify_one();
        recv
    }

    pub fn reattach(&mut self) {
        for report in self.permissions.values() {
            if !self.writing.contains(&report.key)
                && !self.pending.iter().any(|p| p.report.key == report.key)
            {
                self.pending.push_back(PendingReport {
                    report: report.clone(),
                    complete: watch::channel(()).0,
                    confirmed: None,
                });
            }
        }
    }

    fn start(&mut self, prompt: &Value) {
        if let (Some(turn), Some(anchor)) =
            (prompt["turn"].as_str(), prompt["history_index"].as_u64())
            && self.started.as_deref() != Some(turn)
        {
            self.started = Some(turn.into());
            self.push(Observation::Started {
                turn: turn.into(),
                anchor: json!(anchor),
            });
        }
    }

    pub fn steer_prompt(&self, context: &mut Value, turn: &Value, frame: &Value) -> Result<bool> {
        if context["active"]["turn"] != *turn
            || context["active"]["cancelled"] == true
            || self.finished.as_deref() == turn.as_str()
        {
            return Ok(false);
        }
        let input = frame
            .pointer("/params/prompt/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Domain("ACP steering prompt requires text".into()))?;
        let prompt = &mut context["active"];
        let end = prompt["history_end"]
            .as_u64()
            .ok_or_else(|| Error::Domain("Missing ACP prompt history boundary".into()))?;
        // agy completes the superseded request as soon as the next prompt arrives.
        prompt["request_id"] = frame["id"].clone();
        prompt["history_end"] = json!(end + input.len() as u64);
        Ok(true)
    }

    fn finish_original(&mut self, context: &Value, reason: String) {
        let prompt = &context["active"];
        let Some(turn) = prompt["turn"].as_str() else {
            return;
        };
        if self.finished.as_deref() == Some(turn) {
            return;
        }
        self.start(prompt);
        self.finished = Some(turn.into());
        self.permissions
            .retain(|_, p| self.writing.contains(&p.key));
        let pending = context["interjections"]
            .as_object()
            .is_some_and(|entries| entries.values().any(|e| e["turn"] == turn));
        let anchor = prompt["prompt_index"]
            .as_u64()
            .filter(|_| pending)
            .map_or_else(|| prompt["history_end"].clone(), |i| json!({"prompt": i+1}));
        self.push(Observation::Finished {
            turn: turn.into(),
            anchor,
            reason,
        });
    }

    fn observe_promotion(&mut self, context: &Value, frame: &Value) {
        let params = &frame["params"];
        let method = &frame["method"];
        let protocol = &context["protocol"];
        if context["interject"] != true
            || params["sessionId"] != context["native"]
            || params["_meta"]["isReplay"] == true
        {
            return;
        }
        if method == &protocol["queue"]
            && params["runningKind"] == "prompt"
            && let (Some(id), Some(text), Some(prefix)) = (
                params["runningPromptId"].as_str(),
                params["runningText"].as_str(),
                protocol["promoted_prefix"].as_str(),
            )
        {
            if !id.starts_with(prefix) {
                self.original_native = Some(id.into());
                self.original_terminal = None;
                if let Some(index) = context["active"]["prompt_index"].as_u64() {
                    self.next_index = index as usize + 1;
                }
            } else if self.promotion.as_ref().is_none_or(|p| p.id != id) {
                let index = self.next_index;
                self.next_index += 1;
                self.promotion = Some(Promotion {
                    id: id.into(),
                    digest: crate::steering::text_digest(text),
                    summary: text.chars().take(200).collect(),
                    index,
                    started: false,
                });
                self.push(Observation::PromotionPending { native: id.into() });
            }
            return;
        }
        let update = &params["update"];
        let terminal =
            method == &protocol["terminal"] && update["sessionUpdate"] == "turn_completed";
        if terminal
            && update["prompt_id"]
                .as_str()
                .is_some_and(|id| Some(id) == self.original_native.as_deref())
            && let (Some(turn), Some(reason)) = (
                context["active"]["turn"].as_str(),
                update["stop_reason"].as_str(),
            )
        {
            self.original_terminal = Some((turn.into(), reason.into()));
            return;
        }
        let Some(Promotion {
            id,
            digest,
            summary,
            index,
            started,
        }) = self.promotion.as_mut()
        else {
            return;
        };
        let activity = method == "session/update"
            && params["_meta"]["promptId"] == *id
            && matches!(
                update["sessionUpdate"].as_str(),
                Some(
                    "agent_message_chunk"
                        | "agent_thought_chunk"
                        | "tool_call"
                        | "tool_call_update"
                        | "plan"
                )
            );
        let completed = terminal && update["prompt_id"] == *id;
        if !activity && !completed {
            return;
        }
        let native = id.clone();
        let index = *index;
        if !*started {
            *started = true;
            let digest = digest.clone();
            let summary = summary.clone();
            if let Some((turn, reason)) = self.original_terminal.take()
                && context["active"]["turn"] == turn
            {
                self.finish_original(context, reason);
            }
            self.push(Observation::PromotionStarted {
                native: native.clone(),
                digest,
                summary,
                index,
            });
        }
        if completed && let Some(reason) = update["stop_reason"].as_str() {
            self.push(Observation::PromotionFinished {
                native,
                reason: reason.into(),
                index,
            });
            self.promotion = None;
        }
    }

    pub fn observe(&mut self, context: &mut Value, frame: &Value) -> bool {
        if self.closed {
            // Outstanding RPCs can still receive replies after the Session closes.
            return frame.get("method").is_none();
        }
        // Close takes effect at its successful response in the native stream.
        if frame.get("method").is_none()
            && let Some(id) = &self.close_request_id
            && frame.get("id") == Some(id)
        {
            self.close_request_id = None;
            self.closed = frame.get("error").is_none()
                && frame.get("result").is_some_and(|result| {
                    agent_client_protocol::v1::schema::CloseSessionResponse::deserialize(result)
                        .is_ok()
                });
            return true;
        }
        if frame["method"] == "session/request_permission" {
            self.push(Observation::Permission {
                frame: frame.clone(),
            });
            return false;
        }
        if frame["method"] == "$/cancel_request" {
            let native = frame["params"]["requestId"].to_string();
            if let Some(report) = self.permissions.remove(&native)
                && let Observation::Permission { frame } = report.observation
            {
                self.push(Observation::Cancelled {
                    registration: report.key,
                    digest: permission_digest(&frame),
                });
            }
        }
        if context["ready"] != true {
            return true;
        }
        if !context["protocol"].is_null() && frame["params"]["_meta"]["isReplay"] == true {
            return true;
        }
        if context["interject"] == true
            && frame.pointer("/error/code").and_then(Value::as_i64) == Some(-32601)
            && frame.get("method").is_none()
            && let Some(requests) = context["interjections"].as_object_mut()
            && let Some(id) = requests
                .iter()
                .find(|(_, v)| v["request_id"] == frame["id"])
                .map(|(id, _)| id.clone())
        {
            requests.remove(&id);
            context["interject"] = json!(false);
        }
        self.observe_promotion(context, frame);
        let prompt = context["active"].clone();
        let update = &frame["params"]["update"];
        if context["compaction_events"] == true
            && let Some(observation) = match context["compaction_protocol"].as_str() {
                Some("grok") => crate::compaction::observe(context, frame),
                Some("kiro") => {
                    crate::kiro::compaction(context, frame, &mut self.automatic_compaction)
                }
                _ => None,
            }
        {
            self.push(observation);
        }
        let own_update = frame["method"] == "session/update"
            && frame["params"]["sessionId"] == context["native"];
        if own_update {
            match update["sessionUpdate"].as_str() {
                Some("available_commands_update") if context["compaction_protocol"] == "grok" => {
                    if let Some(commands) = update["availableCommands"].as_array() {
                        context["compact_command"] =
                            json!(commands.iter().any(|cmd| cmd["name"] == "compact"));
                    }
                }
                Some("session_info_update") if context["compaction_protocol"] == "kiro" => {
                    if let Some(info) = crate::kiro::info(update)
                        && info["replay"] != true
                        && let Some(usage) = crate::kiro::usage(context, info)
                    {
                        self.push(usage);
                    }
                }
                Some("usage_update") => {
                    if let (Some(used), Some(size)) =
                        (update["used"].as_u64(), update["size"].as_u64())
                        && size > 0
                    {
                        self.push(Observation::Usage { used, size });
                    }
                }
                Some(
                    "agent_message_chunk"
                    | "agent_thought_chunk"
                    | "tool_call"
                    | "tool_call_update"
                    | "plan",
                ) if prompt["request_id"] != Value::Null => self.start(&prompt),
                _ => {}
            }
        }
        let options = if own_update {
            update.get("configOptions")
        } else {
            frame.pointer("/result/configOptions")
        };
        if let Some(options) = options
            && let Ok(options) = serde_json::from_value::<
                Vec<agent_client_protocol::v1::schema::SessionConfigOption>,
            >(options.clone())
            && let Some(model) = crate::models::current(&options)
            && context["current_model"] != model
        {
            context["current_model"] = json!(model);
            self.push(Observation::Model { model });
        }
        if !prompt.is_null()
            && frame.get("method").is_none()
            && frame["id"] == prompt["request_id"]
            && let Some(reason) = frame.pointer("/result/stopReason").and_then(Value::as_str)
        {
            self.finish_original(context, reason.into());
        }
        true
    }

    pub fn permission(&self, id: &Value, key: &str) -> bool {
        self.permissions
            .get(&id.to_string())
            .is_some_and(|p| p.key == key)
    }

    pub fn begin_write(&mut self, id: &Value, key: &str) -> Option<Report> {
        if !self.permission(id, key) || !self.writing.insert(key.into()) {
            return None;
        }
        self.permissions.get(&id.to_string()).cloned()
    }

    pub fn flushed(&mut self, id: Value, report: Report, selected: bool) {
        self.writing.remove(&report.key);
        if self.permission(&id, &report.key) {
            self.permissions.remove(&id.to_string());
        }
        if let Observation::Permission { frame } = report.observation {
            let registration = report.key;
            let digest = permission_digest(&frame);
            self.push(if selected {
                Observation::Delivered {
                    registration,
                    digest,
                }
            } else {
                Observation::Cancelled {
                    registration,
                    digest,
                }
            });
        }
    }
}

pub(crate) fn permission_digest(frame: &Value) -> String {
    pontia_agent_control::interactions::authorization_digest(json!([frame["id"], frame["params"]]))
}

pub(super) async fn drain(
    peer: &Arc<RpcPeer>,
    state: &Mutex<State>,
    input: &Mutex<ChildStdin>,
) -> Result<()> {
    let wake = state.lock().await.reports.wake.clone();
    let mut delay = Duration::from_millis(250);
    loop {
        let next = state
            .lock()
            .await
            .reports
            .pending
            .front()
            .map(|p| p.report.clone());
        let Some(report) = next else {
            wake.notified().await;
            continue;
        };
        let receipt = match &report.observation {
            Observation::Permission { frame } => {
                send_frame(
                    peer,
                    report
                        .key
                        .strip_prefix("acp:frame:")
                        .expect("permission identity"),
                    frame,
                )
                .await?
            }
            _ => {
                peer.request(
                    "relay.report",
                    serde_json::to_value(&report)?,
                    MAX_FRAME_BYTES,
                    false,
                )
                .await?
            }
        };
        if receipt.pointer("/error/code").and_then(Value::as_i64) == Some(-32603) {
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(5));
            continue;
        }
        if receipt.pointer("/result/ended") == Some(&Value::Bool(true))
            && let Observation::Permission { frame } = &report.observation
        {
            let mut stdin = input.lock().await;
            let id = &frame["id"];
            if state
                .lock()
                .await
                .reports
                .begin_write(id, &report.key)
                .is_some()
            {
                let mut bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,
                    "result":{"outcome":{"outcome":"cancelled"}}}))?;
                bytes.push(b'\n');
                stdin.write_all(&bytes).await?;
                stdin.flush().await?;
                let mut s = state.lock().await;
                s.reports.writing.remove(&report.key);
                if s.reports.permission(id, &report.key) {
                    s.reports.permissions.remove(&id.to_string());
                }
            }
        }
        let result = if receipt.get("error").is_some() {
            eprintln!("Discarding rejected ACP report {}", report.key);
            Err(Error::StateConflict(format!(
                "ACP report rejected: {}",
                report.key
            )))
        } else {
            Ok(())
        };
        let mut state = state.lock().await;
        let completed = state
            .reports
            .pending
            .pop_front()
            .expect("only the sender removes reports");
        if result.is_err()
            && let Observation::Permission { frame } = &report.observation
            && state.reports.permission(&frame["id"], &report.key)
        {
            state.reports.permissions.remove(&frame["id"].to_string());
        }
        if let Some(send) = completed.confirmed {
            let _ = send.send(result);
        }
        delay = Duration::from_millis(250);
    }
}

pub(super) async fn send_frame(peer: &Arc<RpcPeer>, id: &str, frame: &Value) -> Result<Value> {
    let mut bytes = serde_json::to_vec(frame)?;
    bytes.push(b'\n');
    let count = bytes.len().div_ceil(64 * 1024);
    let mut receipt = Value::Null;
    for (index, chunk) in bytes.chunks(64 * 1024).enumerate() {
        receipt = peer
            .request(
                "relay.frame",
                json!({"frame_id":id,"chunk":STANDARD.encode(chunk),"last":index+1==count}),
                MAX_FRAME_BYTES,
                false,
            )
            .await?;
        if receipt.get("error").is_some() {
            break;
        }
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_prompt_refuses_steering_before_controller_receives_completion() {
        let mut reports = Reports::default();
        let mut context = json!({"ready":true,"active":{
            "turn":"turn","request_id":1,"history_index":0,"history_end":5
        }});
        reports.observe(
            &mut context,
            &json!({"id":1,"result":{"stopReason":"end_turn"}}),
        );
        let late = json!({"id":2,"params":{"prompt":[{"type":"text","text":"late"}]}});
        assert!(
            !reports
                .steer_prompt(&mut context, &json!("turn"), &late)
                .unwrap()
        );
        assert_eq!(context["active"]["request_id"], 1);
        assert_eq!(context["active"]["history_end"], 5);
    }

    #[test]
    fn kiro_usage_and_summarization_follow_the_native_session_and_operation() {
        let mut reports = Reports::default();
        let mut context = json!({"ready":true, "native":"kiro-session", "compaction_events":true,
            "compaction_protocol":"kiro", "current_model":"haiku", "model_windows":{"haiku":200000}});
        let update = |session: &str, info: Value| {
            json!({"method":"session/update", "params":{
                "sessionId":session, "update":{"sessionUpdate":"session_info_update","_meta":{"kiro":info}}
            }})
        };
        let usage = json!({"kind":"context_usage","usagePercentage":9.5});
        reports.observe(&mut context, &update("foreign", usage.clone()));
        reports.observe(
            &mut context,
            &update(
                "kiro-session",
                json!({"kind":"context_usage", "usagePercentage":80, "replay":true}),
            ),
        );
        reports.observe(&mut context, &update("kiro-session", usage));
        let started = json!({"kind":"summarization_started"});
        let done = json!({"kind":"summarization_completed"});
        reports.observe(&mut context, &update("kiro-session", started));
        reports.observe(&mut context, &update("kiro-session", done.clone()));
        context["compaction"] = json!({"operation":"manual", "request_id":42});
        reports.observe(&mut context, &update("kiro-session", done));
        reports.observe(&mut context, &json!({"id":1,"result":{"success":true}}));
        reports.observe(&mut context, &json!({"id":42,"result":{"success":true}}));
        context["compaction"] = json!({"operation":"rejected", "request_id":43});
        reports.observe(
            &mut context,
            &json!({"id":43,"result":{"success":false,"error":"no history"}}),
        );
        let observations: Vec<_> = reports
            .pending
            .iter()
            .map(|p| &p.report.observation)
            .collect();
        assert!(matches!(observations.as_slice(), [
            Observation::UsageEstimate {ratio:0.095, size:Some(200000)},
            Observation::AutoCompactionStarted {operation:start},
            Observation::Compacted {operation:end,manual:false,..},
            Observation::Compacted {operation,manual:true,..},
            Observation::CompactionFailed {operation:failed,manual:true,message,..},
        ] if start == end && operation == "manual" && failed == "rejected" && message == "no history"));
    }

    #[test]
    fn replay_keeps_original_usage_and_relay_identity() {
        let mut reports = Reports::default();
        let mut context = json!({"ready":true,"native":"native"});
        reports.observe(
            &mut context,
            &json!({
                "method":"session/update",
                "params":{"sessionId":"native","_meta":{"pontia.dev/relay-frame":"agent-value"},
                    "update":{"sessionUpdate":"usage_update","used":9,"size":10}}
            }),
        );
        let original = &reports.pending.front().unwrap().report;
        assert!(original.key.starts_with("acp:frame:"));
        assert!(matches!(
            original.observation,
            Observation::Usage { used: 9, size: 10 }
        ));
        let expected = serde_json::to_value(original).unwrap();
        reports.reattach();
        assert_eq!(reports.pending.len(), 1);
        assert_eq!(
            serde_json::to_value(&reports.pending[0].report).unwrap(),
            expected
        );
    }
}
