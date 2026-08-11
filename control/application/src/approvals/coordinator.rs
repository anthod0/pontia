use std::{collections::HashMap, sync::Arc};

use pontia_core::{
    domain::{EventType, ReportedEvent},
    error::{Error, Result},
};
use serde_json::Value;
use tokio::sync::{Mutex, Notify, oneshot};

use super::ApprovalWaitOutcome;

struct ApprovalWaiter {
    session_id: String,
    _hook_input: Value,
    permission_suggestions: Vec<Value>,
    sender: oneshot::Sender<ApprovalWaitOutcome>,
}

struct ApprovalRequestOwner {
    session_id: String,
    turn_id: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ApprovalAcceptScope {
    Once,
    Always,
    Unknown,
}

impl ApprovalAcceptScope {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Always => "always",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Default)]
struct ApprovalCoordinatorState {
    active_requests: HashMap<String, ApprovalRequestOwner>,
    waiters: HashMap<String, ApprovalWaiter>,
    web_accept_scopes: HashMap<String, ApprovalAcceptScope>,
}

#[derive(Clone, Default)]
pub struct ApprovalCoordinator {
    state: Arc<Mutex<ApprovalCoordinatorState>>,
    changed: Arc<Notify>,
    pub(super) finalization: Arc<Mutex<()>>,
}

impl ApprovalCoordinator {
    pub(super) async fn register(
        &self,
        request_event_id: String,
        session_id: String,
        turn_id: String,
        hook_input: Value,
        permission_suggestions: Vec<Value>,
    ) -> Result<oneshot::Receiver<ApprovalWaitOutcome>> {
        loop {
            let changed = self.changed.notified();
            let mut state = self.state.lock().await;
            if !state
                .active_requests
                .values()
                .any(|owner| owner.session_id == session_id && owner.turn_id == turn_id)
            {
                let (sender, receiver) = oneshot::channel();
                state.active_requests.insert(
                    request_event_id.clone(),
                    ApprovalRequestOwner {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                    },
                );
                state.waiters.insert(
                    request_event_id,
                    ApprovalWaiter {
                        session_id,
                        _hook_input: hook_input,
                        permission_suggestions,
                        sender,
                    },
                );
                return Ok(receiver);
            }
            drop(state);
            changed.await;
        }
    }

    pub(super) async fn remove(&self, request_event_id: &str) {
        let mut state = self.state.lock().await;
        state.active_requests.remove(request_event_id);
        state.waiters.remove(request_event_id);
        state.web_accept_scopes.remove(request_event_id);
        drop(state);
        self.changed.notify_waiters();
    }

    pub(super) async fn resolve_request(&self, request_event_id: &str) {
        let mut state = self.state.lock().await;
        state.active_requests.remove(request_event_id);
        state.web_accept_scopes.remove(request_event_id);
        if let Some(waiter) = state.waiters.remove(request_event_id) {
            let _ = waiter.sender.send(ApprovalWaitOutcome::ResolvedElsewhere);
        }
        drop(state);
        self.changed.notify_waiters();
    }

    pub(super) async fn hook_accept_scope(
        &self,
        request_event_id: &str,
    ) -> Option<ApprovalAcceptScope> {
        self.state
            .lock()
            .await
            .web_accept_scopes
            .get(request_event_id)
            .copied()
    }

    pub(super) async fn deliver_decision(
        &self,
        request_event_id: &str,
        session_id: &str,
        decision: ApprovalWaitOutcome,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        let waiter = state.waiters.get(request_event_id).ok_or_else(|| {
            Error::StateConflict("Approval request is no longer actionable".to_string())
        })?;
        if waiter.session_id != session_id {
            return Err(Error::StateConflict(
                "Approval request does not belong to the target Session".to_string(),
            ));
        }
        let decision = if let ApprovalWaitOutcome::AlwaysAllow {
            permission_suggestion,
        } = &decision
        {
            let original = waiter
                .permission_suggestions
                .iter()
                .find(|suggestion| *suggestion == permission_suggestion)
                .cloned()
                .ok_or_else(|| {
                    Error::StateConflict(
                        "permission suggestion does not exactly match the pending Approval"
                            .to_string(),
                    )
                })?;
            ApprovalWaitOutcome::AlwaysAllow {
                permission_suggestion: original,
            }
        } else {
            decision
        };
        let web_accept_scope = match &decision {
            ApprovalWaitOutcome::AcceptOnce => Some(ApprovalAcceptScope::Once),
            ApprovalWaitOutcome::AlwaysAllow { .. } => Some(ApprovalAcceptScope::Always),
            ApprovalWaitOutcome::ResolvedElsewhere | ApprovalWaitOutcome::Reject => None,
        };
        let waiter = state
            .waiters
            .remove(request_event_id)
            .expect("checked waiter must still exist while coordinator lock is held");
        if let Some(scope) = web_accept_scope {
            state
                .web_accept_scopes
                .insert(request_event_id.to_string(), scope);
        }
        if waiter.sender.send(decision).is_err() {
            state.web_accept_scopes.remove(request_event_id);
            return Err(Error::StateConflict(
                "Approval request is no longer actionable".to_string(),
            ));
        }
        Ok(())
    }

    pub async fn resolve_terminal_event(&self, event: &ReportedEvent) {
        let turn_terminal = matches!(
            event.event_type,
            EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted
        );
        if !turn_terminal && event.event_type != EventType::SessionExited {
            return;
        }

        let matching_ids = {
            let state = self.state.lock().await;
            state
                .active_requests
                .iter()
                .filter(|(_, owner)| {
                    owner.session_id == event.session_id
                        && (event.event_type == EventType::SessionExited
                            || event.turn_id.as_deref() == Some(owner.turn_id.as_str()))
                })
                .map(|(request_event_id, _)| request_event_id.clone())
                .collect::<Vec<_>>()
        };
        let mut state = self.state.lock().await;
        for request_event_id in matching_ids {
            state.active_requests.remove(&request_event_id);
            state.web_accept_scopes.remove(&request_event_id);
            if let Some(waiter) = state.waiters.remove(&request_event_id) {
                let _ = waiter.sender.send(ApprovalWaitOutcome::ResolvedElsewhere);
            }
        }
        drop(state);
        self.changed.notify_waiters();
    }
}
