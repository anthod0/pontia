use crate::transport::Connection;
use pontia_application::{
    EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact,
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use tokio::sync::Mutex;

pub(crate) struct Input {
    pub id: String,
    pub inbox: Option<String>,
    pub summary: String,
    pub turn: Option<String>,
    pub sequence: u64,
}
#[derive(Default)]
pub(crate) struct State {
    pub unavailable: Option<String>,
    pub closing: bool,
    pub closed: bool,
    pub native: Option<String>,
    pub input: Option<Input>,
}
pub(crate) struct Peer {
    pub session: String,
    pub runtime: String,
    pub cwd: PathBuf,
    pub config_dir: PathBuf,
    pub state: Mutex<State>,
    pub connection: OnceLock<Arc<Connection>>,
}
impl Peer {
    pub async fn unavailable(&self, reason: &str) {
        let mut state = self.state.lock().await;
        if !state.closed && state.unavailable.is_none() {
            state.unavailable = Some(reason.into());
        }
    }
    pub fn connection(&self) -> Result<&Arc<Connection>> {
        self.connection
            .get()
            .ok_or_else(|| Error::CapabilityUnavailable("Claude Agent host is unavailable".into()))
    }
    pub async fn runtime_event(
        &self,
        events: &EventIngestService,
        kind: PontiaEventType,
        mut data: Value,
    ) -> Result<()> {
        data["runtime_id"] = json!(self.runtime);
        data["role"] = json!("claude_sdk_host");
        events
            .ingest_pontia_event(PontiaEvent::new(
                &self.session,
                None,
                PontiaEventSource::RuntimeManager,
                "claude-agent",
                kind,
                data,
            ))
            .await?;
        Ok(())
    }
    pub async fn event(
        &self,
        events: &EventIngestService,
        host: &str,
        event: &Value,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        match event["kind"].as_str() {
            Some("ready") => {
                let native = field(event, "native_session_id")?;
                if state.native.as_deref().is_some_and(|id| id != native) {
                    return Err(invalid());
                }
                events.report_client_fact(&self.session, Some(&self.runtime), EventType::SessionReady,
                    json!({"client_session_key":native,"launch_cwd":self.cwd,"binding_metadata":{"claude_config_dir":self.config_dir}})).await?;
                self.runtime_event(events, PontiaEventType::RuntimeReady, json!({}))
                    .await?;
                state.native = Some(native.into());
            }
            Some("closed") => {
                state.closed = true;
            }
            Some("unavailable") => {
                // Keep only the host's bounded diagnostics, never SDK payloads or credentials.
                state.unavailable = Some(field(event, "reason")?.into());
                tracing::warn!(session = %self.session, reason = ?state.unavailable, result = ?event.get("result"), "Claude Agent execution unavailable");
            }
            Some(kind @ ("started" | "finished" | "text" | "tool" | "tool_result")) => {
                if state.closed {
                    return Err(invalid());
                }
                let input = state.input.as_mut().ok_or_else(invalid)?;
                if event["input_id"] != input.id {
                    return Err(invalid());
                }
                if kind == "started" {
                    if input.turn.is_some() {
                        return Err(invalid());
                    }
                    let result = events.report_fact(ReportedFact {
                        session_id: self.session.clone(), turn_id: None,
                        report_key: Some(format!("claude:{host}:{}:start", input.id)), fact_type: EventType::TurnStarted,
                        data: json!({"runtime_id":self.runtime,"claude_input_id":input.id,"input":{"summary":input.summary,"inbox_message_id":input.inbox}}),
                    }).await.map_err(|error| Error::Domain(error.to_string()))?;
                    input.turn = Some(result.turn_id.ok_or_else(invalid)?);
                    input.summary.clear();
                } else if kind == "finished" {
                    let turn = input.turn.as_ref().ok_or_else(invalid)?;
                    let kind = match event["outcome"].as_str() {
                        Some("completed") => EventType::TurnCompleted,
                        Some("failed") => EventType::TurnFailed,
                        Some("interrupted") => EventType::TurnInterrupted,
                        _ => return Err(invalid()),
                    };
                    events.report_fact(ReportedFact {
                        session_id: self.session.clone(), turn_id: Some(turn.clone()),
                        report_key: Some(format!("claude:{host}:{}", field(event,"result_id")?)), fact_type: kind,
                        data: json!({"runtime_id":self.runtime,"claude_input_id":input.id,"claude_result_id":event["result_id"]}),
                    }).await.map_err(|error| Error::Domain(error.to_string()))?;
                    state.input = None;
                } else {
                    crate::output::publish(events, self, input, event).await?;
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}
pub(crate) fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)
}
fn invalid() -> Error {
    Error::Domain("Claude Agent fact is invalid or unassociated".into())
}
