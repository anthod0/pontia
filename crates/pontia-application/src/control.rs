use pontia_core::{Error, Result};

/// A control acknowledgement is never a lifecycle fact.
#[derive(Debug)]
pub enum ControlResult<T> {
    Accepted(T),
    Sent(T),
    Rejected(Error),
    Unknown(String),
    Unsupported(String),
}

impl<T> ControlResult<T> {
    pub(crate) fn from_result(result: Result<T>) -> Self {
        match result {
            Ok(value) => Self::Accepted(value),
            Err(Error::ControlUnknown(message)) => Self::Unknown(message),
            Err(error) => Self::Rejected(error),
        }
    }

    /// Existing HTTP commands acknowledge the request; their returned projections
    /// continue to describe only committed client facts.
    pub(crate) fn into_result(self) -> Result<T> {
        match self {
            Self::Accepted(value) | Self::Sent(value) => Ok(value),
            Self::Rejected(error) => Err(error),
            Self::Unknown(message) => Err(Error::ControlUnknown(message)),
            Self::Unsupported(message) => Err(Error::CapabilityUnavailable(message)),
        }
    }
}

#[derive(Debug, Default)]
pub struct InputReceipt {
    pub native_turn_id: Option<String>,
    pub runtime_instance_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlCommandOutcome {
    pub data: serde_json::Value,
    pub duplicate: bool,
}
