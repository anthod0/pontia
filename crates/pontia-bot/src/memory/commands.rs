use crate::identity::BotIdentity;
use std::sync::Arc;

use super::{Error, MemoryRequest, MemoryResponse, MemoryService, Result};

#[derive(Clone)]
pub struct MemoryCommands {
    service: MemoryService,
    identity: Arc<dyn BotIdentity>,
}

impl MemoryCommands {
    pub fn new(service: MemoryService, identity: Arc<dyn BotIdentity>) -> Self {
        Self { service, identity }
    }

    pub async fn execute(&self, request: MemoryRequest) -> Result<MemoryResponse> {
        let access = self
            .identity
            .resolve(&request.session_id)
            .await
            .map_err(|_| Error::Forbidden)?;
        self.service.execute(&access, request.command).await
    }
}
