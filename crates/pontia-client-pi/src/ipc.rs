//! Pi registration and reporting transport. Binding and lifecycle decisions remain in application services.
mod live_output;
mod queries;
mod reporting;

use crate::rpc::PiRpcPeer;
use pontia_agent_control::ipc::{identity::NativeAttachment, protocol, rpc::RpcRequest};
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
    let (process_id, peer, mut requests) = pontia_agent_control::ipc::connection(stream);
    let Ok(Some(first)) = tokio::time::timeout(Duration::from_secs(10), requests.recv()).await
    else {
        peer.close();
        return;
    };
    let service = state
        .clients()
        .get("pi")
        .and_then(|registration| registration.service.clone());
    match service {
        Some(service) => {
            let _ = service
                .connect(state, process_id, peer, first, requests)
                .await;
        }
        None => peer.close(),
    }
}

pub(crate) async fn serve_peer(
    state: AgentControl,
    connections: crate::connections::PiConnections,
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
        if let Some(message) = protocol::rejection_message(&request) {
            {
                let _guard = connections.lock_identity().await;
                if let Err(error) = protocol::record_rejection(
                    &state,
                    &request,
                    process_id,
                    Some("pi"),
                    async |session, runtime| connections.connected_to(session, runtime).await,
                )
                .await
                {
                    tracing::warn!(%error, "Could not record Pi control protocol rejection");
                }
            }
            let _ = peer
                .reply_error(request.id, protocol::UNSUPPORTED_VERSION, &message)
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
            let connections = connections.clone();
            let channel = channel.clone();
            let mut identity = registered.clone();
            channel.handling.fetch_add(1, Ordering::SeqCst);
            reads.spawn(async move {
                let _guard = ReplyGuard(&channel);
                let result =
                    dispatch(&state, &connections, &channel, &request, &mut identity).await;
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
        let result = dispatch(&state, &connections, &channel, &request, &mut registered).await;
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
                Error::StateConflict(_) | Error::Conflict { .. } | Error::Unsupported(_) => -32009,
                Error::Domain(_) => -32009,
                Error::Serialization(_) => -32602,
                _ => -32603,
            };
            let message = match &error {
                Error::Domain(message)
                | Error::StateConflict(message)
                | Error::NotFound(message)
                | Error::Conflict { message, .. }
                | Error::Unsupported(message)
                | Error::Unavailable(message) => message.clone(),
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
    connections: &crate::connections::PiConnections,
    channel: &Arc<PiChannel>,
    request: &RpcRequest,
    registered: &mut Option<NativeAttachment>,
) -> Result<Value> {
    let identity_guard = connections.lock_identity().await;
    let ready_report = request.method == "event.report"
        && request
            .params
            .pointer("/event/type")
            .and_then(Value::as_str)
            == Some("session.ready");
    if let Some(identity) = registered.as_ref() {
        let current: Arc<dyn crate::connections::ControlChannel> = channel.clone();
        connections
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
        drop(identity_guard);
        let replay = state.branch_replays().resolve_command(query).await?;
        let _guard = connections.lock_identity().await;
        let current: Arc<dyn crate::connections::ControlChannel> = channel.clone();
        connections
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
        connections.refresh_session(&identity.session_id).await;
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
    connections
        .attach_locked(
            &identity_guard,
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

impl crate::connections::ControlChannel for PiChannel {
    fn set_reasoning_effort<'a>(
        &'a self,
        reasoning_effort: &'a str,
    ) -> crate::connections::ControlOperation<'a> {
        Box::pin(self.peer.set_reasoning_effort(reasoning_effort))
    }
    fn compact<'a>(&'a self, operation_id: &'a str) -> crate::connections::ControlOperation<'a> {
        self.peer.compact(operation_id)
    }
    fn native_history(&self, params: Value) -> crate::connections::ControlOperation<'_, Value> {
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
    fn interrupt(&self) -> pontia_agent_control::DeliveryOperation<'_> {
        self.peer.interrupt()
    }
    fn shutdown(&self) -> crate::connections::ControlOperation<'_> {
        self.peer.shutdown()
    }
    fn shutdown_if_idle(&self) -> crate::connections::ControlOperation<'_, bool> {
        self.peer.shutdown_if_idle()
    }
    fn replay<'a>(&'a self, replay_id: &'a str) -> crate::connections::ControlOperation<'a> {
        self.peer.replay(replay_id)
    }
    fn start_turn<'a>(
        &'a self,
        payload: &'a pontia_agent_control::InputPayload,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        self.peer.start_turn(payload)
    }
    fn steer<'a>(
        &'a self,
        payload: &'a pontia_agent_control::InputPayload,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        crate::connections::ControlChannel::steer(self.peer.as_ref(), payload)
    }
}
