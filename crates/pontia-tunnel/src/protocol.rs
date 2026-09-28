use serde::{Deserialize, Serialize};

use crate::Result;

pub const MAX_MESSAGE_BYTES: usize = 4096;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Message {
    Ping { nonce: [u8; 32] },
    Pong { nonce: [u8; 32] },
}

pub fn nonce() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes)
}
