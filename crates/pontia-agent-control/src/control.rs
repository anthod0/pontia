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
    pub(crate) fn from_delivery(result: std::result::Result<T, crate::DeliveryError>) -> Self {
        match result {
            Ok(value) => Self::Accepted(value),
            Err(crate::DeliveryError::Unsupported(message)) => Self::Unsupported(message),
            Err(crate::DeliveryError::Offline(message)) => {
                Self::Rejected(Error::CapabilityUnavailable(message))
            }
            Err(crate::DeliveryError::Rejected(message)) => Self::Rejected(Error::Domain(message)),
            Err(crate::DeliveryError::StateChanged(message)) => {
                Self::Rejected(Error::StateConflict(message))
            }
            Err(crate::DeliveryError::Uncertain(message)) => Self::Unknown(message),
        }
    }

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

    pub(crate) fn into_delivery_result(self) -> std::result::Result<T, crate::DeliveryError> {
        match self {
            Self::Accepted(value) | Self::Sent(value) => Ok(value),
            Self::Rejected(error) => Err(crate::DeliveryError::from(error)),
            Self::Unknown(message) => Err(crate::DeliveryError::Uncertain(message)),
            Self::Unsupported(message) => Err(crate::DeliveryError::Unsupported(message)),
        }
    }
}

#[derive(Debug, Default)]
pub struct InputReceipt {
    pub turn_pending: bool,
    pub native_turn_id: Option<String>,
    pub runtime_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlCommandOutcome {
    pub data: serde_json::Value,
    pub duplicate: bool,
}
