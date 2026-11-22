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
        anchor: usize,
    },
    Finished {
        turn: String,
        anchor: usize,
        reason: String,
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

#[derive(Default)]
pub(super) struct Reports {
    pending: VecDeque<PendingReport>,
    // Original requests are retained only while their native callbacks are waiting.
    permissions: HashMap<String, Report>,
    writing: std::collections::HashSet<String>,
    pub wake: Arc<Notify>,
    started: Option<String>,
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
                anchor: anchor as usize,
            });
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
        let prompt = context["active"].clone();
        let update = &frame["params"]["update"];
        let own_update = frame["method"] == "session/update"
            && frame["params"]["sessionId"] == context["native"];
        if own_update {
            match update["sessionUpdate"].as_str() {
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
                ) => self.start(&prompt),
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
            && let (Some(turn), Some(anchor), Some(reason)) = (
                prompt["turn"].as_str(),
                prompt["history_end"].as_u64(),
                frame.pointer("/result/stopReason").and_then(Value::as_str),
            )
        {
            self.start(&prompt);
            self.permissions
                .retain(|_, p| self.writing.contains(&p.key));
            self.push(Observation::Finished {
                turn: turn.into(),
                anchor: anchor as usize,
                reason: reason.into(),
            });
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
