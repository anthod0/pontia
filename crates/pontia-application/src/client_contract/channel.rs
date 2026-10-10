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
    fn available(&self) -> bool;
    fn process_id(&self) -> Option<u32> {
        None
    }
    fn invalidate(&self);
    fn list_models(&self) -> ClientControlOperation<'_, Vec<crate::sessions::SessionModel>>;
    fn set_model<'a>(&'a self, model: &'a str) -> ClientControlOperation<'a>;
    fn interrupt(&self) -> ClientControlOperation<'_>;
    fn shutdown(&self) -> ClientControlOperation<'_>;
    fn ping(&self) -> ClientControlOperation<'_>;
    fn replay<'a>(&'a self, inbox_message_id: &'a str) -> ClientControlOperation<'a>;
    fn submit<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
    ) -> ClientControlOperation<'a>;
}
