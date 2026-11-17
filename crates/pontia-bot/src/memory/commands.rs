use std::{future::Future, pin::Pin, sync::Arc};

use super::{Error, MemoryAccess, MemoryRequest, MemoryResponse, MemoryService, Result};

pub trait MemoryIdentity: Send + Sync {
    fn resolve<'a>(
        &'a self,
        session_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<MemoryAccess>> + Send + 'a>>;
}

/// No Session has memory privileges until the Bot application binds its identity.
pub struct UnboundMemoryIdentity;

impl MemoryIdentity for UnboundMemoryIdentity {
    fn resolve<'a>(
        &'a self,
        _session_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<MemoryAccess>> + Send + 'a>> {
        Box::pin(async { Err(Error::Forbidden) })
    }
}

#[derive(Clone)]
pub struct MemoryCommands {
    service: MemoryService,
    identity: Arc<dyn MemoryIdentity>,
}

impl MemoryCommands {
    pub fn new(service: MemoryService, identity: Arc<dyn MemoryIdentity>) -> Self {
        Self { service, identity }
    }

    pub async fn execute(&self, request: MemoryRequest) -> Result<MemoryResponse> {
        let access = self.identity.resolve(&request.session_id).await?;
        self.service.execute(&access, request.command).await
    }
}
