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
        let mutation = matches!(
            request.command,
            super::MemoryCommand::Create(_)
                | super::MemoryCommand::Update(_)
                | super::MemoryCommand::Archive(_)
                | super::MemoryCommand::Link(_)
                | super::MemoryCommand::Unlink(_)
                | super::MemoryCommand::Organize { .. }
        );
        let dreaming_change = access.role == crate::identity::BotRole::Dreaming && mutation;
        if dreaming_change
            && !request.reason.as_ref().is_some_and(|reason| {
                !reason.trim().is_empty() && reason.len() <= 16384 && !reason.contains('\0')
            })
        {
            return Err(Error::InvalidInput(
                "Dreaming changes require a nonblank reason of at most 16384 bytes".into(),
            ));
        }
        let result = self.service.execute(&access, request.command.clone()).await;
        if dreaming_change {
            tracing::info!(session_id = %request.session_id, command = ?request.command, reason = ?request.reason, result = ?result, "Dreaming memory change");
        }
        result
    }
}
