//! Pi registration and reporting transport. Binding and lifecycle decisions remain in application services.
mod live_output;
mod queries;
mod reporting;

use crate::rpc::PiRpcPeer;
use pontia_agent_control::ipc::{identity::NativeAttachment, rpc::RpcRequest};
use pontia_agent_control::{AgentBindingService, AgentControl};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{net::UnixStream, task::JoinSet};

struct ClosePeer(Arc<PiRpcPeer>);
impl Drop for ClosePeer {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub async fn serve_connection(state: AgentControl, stream: UnixStream) {
    let (process_id, peer, requests) = pontia_agent_control::ipc::connection(stream);
    serve_peer(state, process_id, PiRpcPeer::from_rpc(peer), None, requests).await;
}

pub(crate) async fn serve_peer(
    state: AgentControl,
    process_id: Option<u32>,
    peer: Arc<PiRpcPeer>,
    mut first: Option<RpcRequest>,
    mut requests: pontia_agent_control::ipc::rpc::Requests,
) {
    let _close = ClosePeer(peer.clone());
    let channel = Arc::new(PiChannel {
        process_id,
        peer: peer.clone(),
        handling: AtomicUsize::new(0),
        invalidated: AtomicBool::new(false),
        reasoning_effort_controllable: AtomicBool::new(false),
    });
    let mut registered = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut reads = JoinSet::new();
    loop {
        let routed = first.is_some();
        let request = if let Some(first) = first.take() {
            Some(first)
        } else {
            tokio::select! {
                biased;
                _ = peer.closed() => break,
                _ = tokio::time::sleep_until(deadline), if registered.is_none() => break,
                Some(_) = reads.join_next(), if !reads.is_empty() => continue,
                request = requests.recv() => request,
            }
        };
        let Some(request) = request else { break };
        if !routed
            && let Some(message) =
                pontia_agent_control::ipc::protocol::rejection(&state, process_id, &request).await
        {
            let _ = peer
                .reply_error(
                    request.id,
                    pontia_agent_control::ipc::protocol::UNSUPPORTED_VERSION,
                    &message,
                )
                .await;
            break;
        }
        if request.id.is_none() {
            continue;
        } // All Pi reports and registration require an acknowledged response.
        if !matches!(
            request.method.as_str(),
            "session.get"
                | "workspaces.list"
                | "session.context"
                | "runtime.register"
                | "runtime.attach"
                | "event.report"
                | "liveOutput.publish"
                | "branch.resolve"
        ) {
            if peer
                .reply_error(request.id, -32601, "Unknown Pi RPC method")
                .await
                .is_err()
            {
                break;
            }
            continue;
        }
        if request.method == "branch.resolve" {
            if reads.len() >= 16 {
                if peer
                    .reply_error(request.id, -32009, "Too many outstanding history reads")
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            let state = state.clone();
            let channel = channel.clone();
            let mut identity = registered.clone();
            channel.handling.fetch_add(1, Ordering::SeqCst);
            reads.spawn(async move {
                let _guard = ReplyGuard(&channel);
                let result = dispatch(&state, &channel, &request, &mut identity).await;
                if reply_result(&channel.peer, request.id, result)
                    .await
                    .is_err()
                {
                    channel.peer.close();
                }
            });
            continue;
        }
        channel.handling.fetch_add(1, Ordering::SeqCst);
        let reply_guard = ReplyGuard(&channel);
        let result = dispatch(&state, &channel, &request, &mut registered).await;
        let response = reply_result(&peer, request.id, result).await;
        drop(reply_guard);
        if response.is_err() {
            break;
        }
    }
}

async fn reply_result(peer: &PiRpcPeer, id: Option<Value>, result: Result<Value>) -> Result<()> {
    match result {
        Ok(value) => peer.reply(id, value).await,
        Err(error) => {
            let code = match &error {
                Error::NotFound(_) => -32004,
                Error::StateConflict(_) | Error::CapabilityUnavailable(_) => -32009,
                Error::Domain(_) => -32009,
                Error::Serialization(_) => -32602,
                _ => -32603,
            };
            let message = match &error {
                Error::Domain(message)
                | Error::StateConflict(message)
                | Error::NotFound(message)
                | Error::CapabilityUnavailable(message) => message.clone(),
                _ => error.to_string(),
            };
            peer.reply_error(id, code, &message).await
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextQuery {
    client_session_key: String,
}

async fn dispatch(
    state: &AgentControl,
    channel: &Arc<PiChannel>,
    request: &RpcRequest,
    registered: &mut Option<NativeAttachment>,
) -> Result<Value> {
    let _identity_guard = state.client_control().lock_identity().await;
    let ready_report = request.method == "event.report"
        && request
            .params
            .pointer("/event/type")
            .and_then(Value::as_str)
            == Some("session.ready");
    if let Some(identity) = registered.as_ref() {
        let current: Arc<dyn pontia_agent_control::ClientControlChannel> = channel.clone();
        state
            .client_control()
            .validate_connection(&identity.session_id, &identity.runtime_id, &current)
            .await?;
    }
    if matches!(request.method.as_str(), "session.get" | "workspaces.list") {
        return queries::dispatch(state, request).await;
    }
    if request.method == "liveOutput.publish" {
        let identity = registered
            .as_ref()
            .ok_or_else(|| Error::StateConflict("Pi live output requires registration".into()))?;
        return live_output::publish(state, identity, request.params.clone()).await;
    }
    if request.method == "branch.resolve" {
        let identity = registered.as_ref().ok_or_else(|| {
            Error::StateConflict("Pi branch resolution requires registration".into())
        })?;
        let query: pontia_agent_control::ResolveBranchReplayRequest =
            serde_json::from_value(request.params.clone())?;
        if query.session_id != identity.session_id
            || query.runtime_id != identity.runtime_id
            || query.client_type != "pi"
        {
            return Err(Error::StateConflict(
                "Pi branch resolution does not match its connection identity".into(),
            ));
        }
        // Reverse history calls must not hold the fact/connection identity gate.
        drop(_identity_guard);
        let replay = pontia_agent_control::BranchReplayService::new(state.db())
            .with_clients(state.clients())
            .with_history_control(state.client_control().clone())
            .resolve_command(query)
            .await?;
        let _guard = state.client_control().lock_identity().await;
        let current: Arc<dyn pontia_agent_control::ClientControlChannel> = channel.clone();
        state
            .client_control()
            .validate_connection(&identity.session_id, &identity.runtime_id, &current)
            .await?;
        return Ok(json!({"branch_replay": replay}));
    }
    if request.method == "event.report" {
        let identity = registered
            .as_ref()
            .ok_or_else(|| Error::StateConflict("Pi reporting requires registration".into()))?;
        let result = reporting::report_event(state, identity, request.params.clone()).await?;
        if ready_report {
            channel
                .reasoning_effort_controllable
                .store(true, Ordering::SeqCst);
        }
        return Ok(result);
    }
    if request.method == "session.context" {
        let query: ContextQuery = serde_json::from_value(request.params.clone())?;
        if query.client_session_key.trim().is_empty() {
            return Err(Error::Domain("client_session_key is required".into()));
        }
        let bindings = AgentBindingService::new(state.db());
        let context = match bindings
            .session_context_for_client_session(
                "pi",
                crate::SPEC.adapter.control_runtime_role,
                &query.client_session_key,
            )
            .await?
        {
            Some(context) => Some(context),
            None => {
                bindings
                    .pending_session_context(
                        "pi",
                        crate::SPEC.adapter.control_runtime_role,
                        &query.client_session_key,
                        &query.client_session_key,
                    )
                    .await?
            }
        };
        return Ok(json!({"session_context":context}));
    }
    if registered.is_some() {
        return Err(Error::StateConflict(
            "Connection is already registered".into(),
        ));
    }
    let (identity, result) = pontia_agent_control::ipc::identity::native_registration(
        state,
        "pi",
        channel.process_id,
        request,
    )
    .await?;
    state
        .client_control()
        .attach_locked(
            &_identity_guard,
            "pi",
            &identity.session_id,
            &identity.runtime_id,
            &identity.client_session_key,
            channel.clone(),
        )
        .await?;
    let session_state = state
        .queries()
        .session_snapshot(&identity.session_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("session {} not found", identity.session_id)))?
        .state;
    channel.reasoning_effort_controllable.store(
        matches!(
            session_state.as_str(),
            "idle" | "busy" | "waiting_for_interaction"
        ),
        Ordering::SeqCst,
    );

    state
        .inbox_commands()
        .notify_available(&identity.session_id);
    *registered = Some(identity);
    Ok(result)
}

struct PiChannel {
    process_id: Option<u32>,
    peer: Arc<PiRpcPeer>,
    handling: AtomicUsize,
    invalidated: AtomicBool,
    reasoning_effort_controllable: AtomicBool,
}

impl PiChannel {
    fn connected(&self) -> bool {
        !self.invalidated.load(Ordering::SeqCst) && !self.peer.is_closed()
    }
}

// A terminal fact can invalidate control while its reporting acknowledgement is pending.
struct ReplyGuard<'a>(&'a PiChannel);
impl Drop for ReplyGuard<'_> {
    fn drop(&mut self) {
        if self.0.handling.fetch_sub(1, Ordering::SeqCst) == 1
            && self.0.invalidated.load(Ordering::SeqCst)
        {
            self.0.peer.close();
        }
    }
}

impl pontia_agent_control::ClientControlChannel for PiChannel {
    fn set_reasoning_effort<'a>(
        &'a self,
        reasoning_effort: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        self.peer.set_reasoning_effort(reasoning_effort)
    }
    fn compact<'a>(
        &'a self,
        operation_id: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        self.peer.compact(operation_id)
    }
    fn native_history(
        &self,
        params: Value,
    ) -> pontia_agent_control::ClientControlOperation<'_, Value> {
        self.peer.native_history(params)
    }
    fn process_id(&self) -> Option<u32> {
        self.process_id
    }
    fn available(&self) -> bool {
        self.connected()
    }
    fn reasoning_effort_control_available(&self) -> bool {
        self.connected() && self.reasoning_effort_controllable.load(Ordering::SeqCst)
    }
    fn invalidate(&self) {
        self.invalidated.store(true, Ordering::SeqCst);
        if self.handling.load(Ordering::SeqCst) == 0 {
            self.peer.close();
        }
    }
    fn interrupt(&self) -> pontia_agent_control::ClientControlOperation<'_> {
        self.peer.interrupt()
    }
    fn shutdown(&self) -> pontia_agent_control::ClientControlOperation<'_> {
        self.peer.shutdown()
    }
    fn shutdown_if_idle(&self) -> pontia_agent_control::ClientControlOperation<'_, bool> {
        self.peer.shutdown_if_idle()
    }
    fn ping(&self) -> pontia_agent_control::ClientControlOperation<'_> {
        self.peer.ping()
    }
    fn replay<'a>(
        &'a self,
        inbox_message_id: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        self.peer.replay(inbox_message_id)
    }
    fn submit<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        self.peer.submit(input, inbox_message_id)
    }
    fn steer<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
        turn_id: &'a str,
    ) -> pontia_agent_control::ClientControlOperation<'a> {
        self.peer.steer(input, inbox_message_id, turn_id)
    }
}
