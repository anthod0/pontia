use serde::Deserialize;
use std::num::NonZeroU64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BotConfig {
    pub context_token_limit: NonZeroU64,
    pub context_window_percent: u8,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            context_token_limit: NonZeroU64::new(258_000).unwrap(),
            context_window_percent: 80,
        }
    }
}
