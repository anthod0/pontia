use super::{ClientLaunchRequest, DeliveryError, DeliveryOperation, InputPayload};
use crate::{EventIngestService, control::InputReceipt, runtime::ControlTarget};
use pontia_core::Result;
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
use serde_json::Value;
use sqlx::SqlitePool;
use std::{future::Future, path::Path, pin::Pin};

pub type ClientOperation<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientExitMode {
    Unconditional,
    IfIdle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientExitOutcome {
    Busy,
    Requested,
    Confirmed { reason: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionCreationOption {
    pub model_catalog_key: Option<String>,
    pub id: String,
    pub name: String,
    pub client_type: String,
    pub metadata: Value,
    pub version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeSessionFork {
    pub client_session_key: String,
    pub receipt: Value,
}

/// Session-level lifecycle and control operations implemented by an Agent Client integration.
pub trait ClientSession: Send + Sync {
    fn creation_options(&self) -> ClientOperation<'_, Option<Vec<SessionCreationOption>>> {
        Box::pin(async { Ok(None) })
    }
    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>>;
    fn provision_configured<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
        _configuration: Value,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        self.provision(events, root, request)
    }
    fn start_turn<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
        turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt>;
    fn steer<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _payload: &'a InputPayload,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async {
            Err(DeliveryError::Unsupported(
                "This client does not support steer".into(),
            ))
        })
    }
    fn interrupt<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
    ) -> DeliveryOperation<'a> {
        Box::pin(async {
            Err(DeliveryError::Unsupported(
                "This client does not support interrupt".into(),
            ))
        })
    }
    fn replay<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _replay_id: &'a str,
    ) -> DeliveryOperation<'a> {
        Box::pin(async {
            Err(DeliveryError::Unsupported(
                "This client does not support branch replay".into(),
            ))
        })
    }
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome>;
    fn resume<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Session resume is unavailable".into(),
            ))
        })
    }
    fn resume_with_runtime<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        _request: ClientLaunchRequest<'a>,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move {
            self.resume(events, target).await?;
            Ok(None)
        })
    }
    fn validate_resume<'a>(
        &'a self,
        _pool: SqlitePool,
        _target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn fork_session<'a>(
        &'a self,
        _events: EventIngestService,
        _session: &'a str,
        _turn: &'a str,
    ) -> ClientOperation<'a, NativeSessionFork> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Session fork is unavailable".into(),
            ))
        })
    }
    fn adopt_fork<'a>(
        &'a self,
        _events: EventIngestService,
        _session: &'a str,
        _fork: &'a NativeSessionFork,
    ) -> ClientOperation<'a, String> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Session fork is unavailable".into(),
            ))
        })
    }
    fn compact<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Manual compaction is unsupported".into(),
            ))
        })
    }
    fn set_reasoning_effort<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _reasoning_effort: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Reasoning effort control is unsupported".into(),
            ))
        })
    }
    fn capabilities<'a>(
        &'a self,
        _session: &'a str,
        implementation: &'a crate::client_contract::AgentClientCapabilities,
    ) -> ClientOperation<'a, crate::client_contract::AgentClientCapabilities> {
        Box::pin(async move { Ok(implementation.clone()) })
    }
    fn ready(&self, _session: &str, _instance: &str) -> Option<pontia_core::domain::ReportedEvent> {
        None
    }
    fn native_history(
        &self,
        _pool: SqlitePool,
        _binding: crate::client_contract::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<std::sync::Arc<dyn crate::client_contract::native_history::NativeHistory>> {
        None
    }
    fn available<'a>(&'a self, pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool>;
    fn manual_compaction_available<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        self.available(pool, session)
    }
    fn reasoning_effort_control_available<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        self.available(pool, session)
    }
    fn control_connected<'a>(
        &'a self,
        _pool: SqlitePool,
        _session: &'a str,
    ) -> ClientOperation<'a, Option<bool>> {
        Box::pin(async { Ok(None) })
    }
    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails>;
}

pub struct ClientSessionDetails {
    pub data: Value,
}
