use std::{future::Future, pin::Pin};

pub const TOOL_ADDR_ENV: &str = "PONTIA_BOT_API_ADDR";
pub const TOOL_TOKEN_HEADER: &str = "x-pontia-bot-tool-token";
pub const TOOL_TOKEN_ENV: &str = "PONTIA_BOT_TOOL_TOKEN";

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
