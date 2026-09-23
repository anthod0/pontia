use pontia_core::Result;
use std::{future::Future, pin::Pin};

pub type ClientControlOperation<'a, T = ()> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

pub trait ClientControlChannel: Send + Sync {
    fn available(&self) -> bool;
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
