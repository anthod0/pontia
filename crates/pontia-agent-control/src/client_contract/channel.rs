use super::{DeliveryError, DeliveryOperation, InputPayload};
use pontia_core::Result;
use std::{future::Future, pin::Pin};

pub type ClientControlOperation<'a, T = ()> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

pub trait ClientControlChannel: Send + Sync {
    fn native_history(
        &self,
        _params: serde_json::Value,
    ) -> ClientControlOperation<'_, serde_json::Value> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "native history is unsupported".into(),
            ))
        })
    }
    fn compact<'a>(&'a self, _operation_id: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Manual compaction is unsupported".into(),
            ))
        })
    }
    fn set_reasoning_effort<'a>(
        &'a self,
        _reasoning_effort: &'a str,
    ) -> ClientControlOperation<'a> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Reasoning effort control is unsupported".into(),
            ))
        })
    }
    fn reasoning_effort_control_available(&self) -> bool {
        self.available()
    }
    fn available(&self) -> bool;
    fn process_id(&self) -> Option<u32> {
        None
    }
    fn invalidate(&self);
    fn interrupt(&self) -> DeliveryOperation<'_> {
        Box::pin(async {
            Err(DeliveryError::Unsupported(
                "This client does not support interrupt".into(),
            ))
        })
    }
    fn shutdown(&self) -> ClientControlOperation<'_>;
    fn shutdown_if_idle(&self) -> ClientControlOperation<'_, bool> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "Conditional shutdown is unavailable".into(),
            ))
        })
    }
    fn ping(&self) -> ClientControlOperation<'_>;
    fn replay<'a>(&'a self, replay_id: &'a str) -> ClientControlOperation<'a>;
    fn start_turn<'a>(&'a self, payload: &'a InputPayload) -> DeliveryOperation<'a>;
    fn steer<'a>(&'a self, _payload: &'a InputPayload) -> DeliveryOperation<'a> {
        Box::pin(async {
            Err(DeliveryError::Unsupported(
                "This client does not support steer".into(),
            ))
        })
    }
}
