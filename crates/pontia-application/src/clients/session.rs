use crate::{
    EventIngestService, control::InputReceipt, runtime::control_target::ControlTarget,
    sessions::SessionModel, turns::InputIntent,
};
use pontia_core::Result;
use pontia_runtime::RuntimeStartRequest;
use serde_json::Value;
use sqlx::SqlitePool;
use std::{future::Future, path::Path, pin::Pin};

pub type ClientOperation<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Session operations for clients whose execution lifetime is independent of a local pane.
pub trait ClientSession: Send + Sync {
    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()>;
    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        input: &'a str,
        message: Option<&'a str>,
        intent: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt>;
    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        turn: &'a str,
    ) -> ClientOperation<'a, ()>;
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()>;
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
    fn available<'a>(&'a self, pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool>;
    fn open_interface<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
    ) -> ClientOperation<'a, ()>;
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
