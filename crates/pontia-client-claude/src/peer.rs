use crate::transport::Connection;
use pontia_agent_control::{EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tokio::sync::Mutex;

pub(crate) struct Input {
    pub id: String,
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
    pub model: Option<String>,
    pub resume: Option<String>,
    pub state: Mutex<State>,
    pub interactions: pontia_agent_control::interactions::InteractionService,
    pub connection: RwLock<Option<Arc<Connection>>>,
}
impl Peer {
    pub async fn start(
        self: &Arc<Self>,
        events: &EventIngestService,
        root: &std::path::Path,
        script: &std::path::Path,
        environment: &std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        self.runtime_event(events, PontiaEventType::RuntimeStarting, json!({}))
            .await?;
        match crate::transport::launch(self, root, script, environment, events).await {
            Ok(()) => Ok(()),
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
        if reason == "control_connection_lost" {
            let pending = state
                .permissions
                .keys()
                .map(|id| (self.session.clone(), id.clone()))
                .collect();
            self.interactions.await_reconnect(pending).await;
        } else {
            crate::interactions::expire_all(self, &mut state).await;
        }
    }
    pub fn connection(&self) -> Result<Arc<Connection>> {
        self.connection
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::CapabilityUnavailable("Claude Code host is unavailable".into()))
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
        params: &Value,
        stale: bool,
    ) -> Result<Value> {
        let event = &params["event"];
        let mut state = self.state.lock().await;
        if let Some(report) = crate::reports::HostReport::parse(host, params)? {
            return self.report(events, &mut state, host, report, stale).await;
        }
        if stale
            && !matches!(
                event["kind"].as_str(),
                Some("permission_requested" | "permission_settled")
            )
        {
            return Err(Error::StateConflict(
                "Claude host sequence already processed".into(),
            ));
        }
        match event["kind"].as_str() {
            Some("permission_requested" | "permission_settled") => {
                return crate::interactions::event(self, &mut state, host, params).await;
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
                tracing::warn!(session = %self.session, reason = ?state.unavailable, result = ?event.get("result"), "Claude Code execution unavailable");
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
            Some("text" | "input" | "tool" | "tool_result") => {
                if state.closed {
                    return Err(invalid());
                }
                let input = state.input.as_mut().ok_or_else(invalid)?;
                if event["input_id"] != input.id {
                    return Err(invalid());
                }
                crate::output::publish(events, self, input, event).await?;
            }
            _ => return Err(invalid()),
        }
        Ok(json!({"accepted":true}))
    }
}
pub(crate) fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)
}
pub(crate) fn invalid() -> Error {
    Error::Domain("Claude Code fact is invalid or unassociated".into())
}
