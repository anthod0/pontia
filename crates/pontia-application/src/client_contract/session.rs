use crate::{
    EventIngestService, control::InputReceipt, runtime::ControlTarget, sessions::SessionModel,
    turns::InputIntent,
};
use pontia_core::Result;
use pontia_runtime::RuntimeStartRequest;
use serde_json::Value;
use sqlx::SqlitePool;
use std::{future::Future, path::Path, pin::Pin};

pub type ClientOperation<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientExitOutcome {
    Requested,
    Confirmed { reason: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionCreationOption {
    pub id: String,
    pub name: String,
    pub client_type: String,
    pub metadata: Value,
}

/// Session operations for clients whose execution lifetime is independent of a local pane.
pub trait ClientSession: Send + Sync {
    fn creation_options(&self) -> ClientOperation<'_, Option<Vec<SessionCreationOption>>> {
        Box::pin(async { Ok(None) })
    }
    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()>;
    fn provision_configured<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
        _configuration: Value,
    ) -> ClientOperation<'a, ()> {
        self.provision(events, root, request)
    }
    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        input: &'a str,
        turn: Option<&'a str>,
        message: Option<&'a str>,
        intent: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt>;
    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        turn: Option<&'a str>,
    ) -> ClientOperation<'a, ()>;
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ClientExitOutcome>;
    fn resume<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()>;
    fn list_models<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, Vec<SessionModel>>;
    fn set_model<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        model: &'a str,
    ) -> ClientOperation<'a, ()>;
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
    fn capabilities<'a>(
        &'a self,
        _session: &'a str,
        implementation: &'a crate::client_contract::AgentClientCapabilities,
    ) -> ClientOperation<'a, crate::client_contract::AgentClientCapabilities> {
        Box::pin(async move { Ok(implementation.clone()) })
    }
    fn available<'a>(&'a self, pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool>;
    fn manual_compaction_available<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        self.available(pool, session)
    }
    fn open_interface<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        session: &'a str,
        runtime_id: &'a str,
    ) -> ClientOperation<'a, pontia_runtime::RuntimeStartResult>;
    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails>;
}

pub struct ClientSessionDetails {
    pub data: Value,
    pub model_control_unavailable_reason: Option<String>,
}

pub trait InProcessClient: Send + Sync {
    fn capabilities(&self) -> crate::client_contract::AgentClientCapabilities;
    fn submit(&self, input: crate::client_contract::AgentInput) -> Result<()>;
    fn ready(&self, session: &str, instance: &str) -> pontia_core::domain::ReportedEvent;
}
