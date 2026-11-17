use std::{future::Future, pin::Pin};

/// Roles and scope come from trusted Session bindings, never from command input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotRole {
    Bot,
    Helper,
    Retrieval,
    Dreaming,
}

#[derive(Clone, Debug)]
pub struct BotAccess {
    pub role: BotRole,
    pub workspace_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("Session is not authorized for this Bot operation")]
pub struct IdentityError;

pub trait BotIdentity: Send + Sync {
    fn resolve<'a>(
        &'a self,
        session_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<BotAccess, IdentityError>> + Send + 'a>>;
}

/// No Session has Bot privileges until the Bot application binds its identity.
pub struct UnboundBotIdentity;

impl BotIdentity for UnboundBotIdentity {
    fn resolve<'a>(
        &'a self,
        _session_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<BotAccess, IdentityError>> + Send + 'a>>
    {
        Box::pin(async { Err(IdentityError) })
    }
}
