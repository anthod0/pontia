use crate::transport::Connection;
use pontia_application::{
    EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact,
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
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
    pub retry_reason: Option<String>,
    pub closing: bool,
    pub closed: bool,
    pub closed_anchor: Option<String>,
    pub native: Option<String>,
    pub input: Option<Input>,
    pub permissions: HashMap<String, crate::interactions::Permission>,
}
pub(crate) struct Peer {
    pub session: String,
    pub runtime: String,
    pub cwd: PathBuf,
    pub config_dir: PathBuf,
    pub resume: Option<String>,
    pub state: Mutex<State>,
    pub interactions: pontia_application::interactions::InteractionService,
    pub connection: OnceLock<Arc<Connection>>,
}
impl Peer {
    pub async fn start(
        self: &Arc<Self>,
        events: &EventIngestService,
        root: &std::path::Path,
        script: &std::path::Path,
        environment: &std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        let directory = std::path::absolute(root)?
            .join("state/claude")
            .join(uuid::Uuid::new_v4().simple().to_string());
        // Each invocation owns a fresh private socket directory, including resume.
        std::fs::create_dir_all(directory.parent().ok_or_else(crate::unavailable)?)?;
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        self.runtime_event(events, PontiaEventType::RuntimeStarting, json!({}))
            .await?;
        match Connection::spawn(
            self.clone(),
            script,
            &directory.join("control.sock"),
            environment,
            events.clone(),
        )
        .await
        {
            Ok(connection) => {
                let _ = self.connection.set(connection);
                Ok(())
            }
            Err(error) => {
                self.runtime_event(
                    events,
                    PontiaEventType::RuntimeExited,
                    json!({"reason":"host_start_failed"}),
                )
                .await?;
                Err(error)
            }
        }
    }

    pub async fn unavailable(&self, reason: &str) {
        let mut state = self.state.lock().await;
        if !state.closed
            && (state.unavailable.is_none()
                || (reason == "host_unavailable"
                    && state.unavailable.as_deref() == Some("control_connection_lost")))
        {
            state.unavailable = Some(reason.into());
        }
        crate::interactions::expire_all(self, &mut state).await;
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
        if kind == PontiaEventType::RuntimeExited {
            let state = self.state.lock().await;
            if state.closed
                && let Some(input) = &state.input
            {
                data["claude_closed_input"] = json!(input.id);
                data["claude_closed_history_anchor"] = json!(state.closed_anchor);
            }
        }
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
                if state.native.as_deref().is_some_and(|id| id != native)
                    || self.resume.as_deref().is_some_and(|id| id != native)
                {
                    return Err(invalid());
                }
                events.report_client_fact(&self.session, Some(&self.runtime), EventType::SessionReady,
                    json!({"client_session_key":native,"launch_cwd":self.cwd,"binding_metadata":{"claude_config_dir":self.config_dir}})).await?;
                self.runtime_event(events, PontiaEventType::RuntimeReady, json!({}))
                    .await?;
                state.native = Some(native.into());
            }
            Some("permission_requested" | "permission_settled") => {
                crate::interactions::event(self, &mut state, events, host, event).await?;
            }
            Some("closed") => {
                state.closed = true;
                state.retry_reason = None;
                state.closed_anchor = event["history_anchor"].as_str().map(str::to_owned);
                crate::interactions::expire_all(self, &mut state).await;
            }
            Some("unavailable") => {
                // Keep only the host's bounded diagnostics, never SDK payloads or credentials.
                state.unavailable = Some(field(event, "reason")?.into());
                crate::interactions::expire_all(self, &mut state).await;
                tracing::warn!(session = %self.session, reason = ?state.unavailable, result = ?event.get("result"), "Claude Agent execution unavailable");
            }
            Some("retry") => {
                state.retry_reason = match event["reason"].as_str() {
                    Some(reason @ ("authentication_failed_retrying" | "api_request_retrying")) => {
                        Some(reason.into())
                    }
                    None if event["reason"].is_null() => None,
                    _ => return Err(invalid()),
                };
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
                        data: json!({"runtime_id":self.runtime,"claude_input_id":input.id,"claude_history_anchor":input.id,"input":{"summary":input.summary,"inbox_message_id":input.inbox}}),
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
                        data: json!({"runtime_id":self.runtime,"claude_input_id":input.id,"claude_result_id":event["result_id"],"claude_history_anchor":event["history_anchor"]}),
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
