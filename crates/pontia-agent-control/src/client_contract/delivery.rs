use pontia_core::Error;
use std::{future::Future, pin::Pin};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InputPayload {
    pub text: String,
    pub(crate) metadata: serde_json::Value,
    pub(crate) prepared_turn_id: Option<String>,
    pub(crate) force_queue: bool,
}

impl InputPayload {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metadata: serde_json::Value::Object(Default::default()),
            prepared_turn_id: None,
            force_queue: false,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub(crate) fn with_prepared_turn(mut self, turn_id: Option<String>) -> Self {
        self.prepared_turn_id = turn_id;
        self
    }

    pub fn force_queue(mut self) -> Self {
        self.force_queue = true;
        self
    }
}

impl From<String> for InputPayload {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for InputPayload {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeliveryError {
    #[error("delivery unsupported: {0}")]
    Unsupported(String),
    #[error("delivery offline: {0}")]
    Offline(String),
    #[error("delivery state changed: {0}")]
    StateChanged(String),
    #[error("delivery uncertain: {0}")]
    Uncertain(String),
}

impl From<Error> for DeliveryError {
    fn from(error: Error) -> Self {
        match error {
            Error::CapabilityUnavailable(message) => Self::Offline(message),
            Error::ControlUnknown(message) => Self::Uncertain(message),
            Error::StateConflict(message) | Error::Conflict { message, .. } => {
                Self::StateChanged(message)
            }
            error => Self::Offline(error.to_string()),
        }
    }
}

impl From<DeliveryError> for Error {
    fn from(error: DeliveryError) -> Self {
        match error {
            DeliveryError::Unsupported(message) | DeliveryError::Offline(message) => {
                Self::CapabilityUnavailable(message)
            }
            DeliveryError::StateChanged(message) => Self::StateConflict(message),
            DeliveryError::Uncertain(message) => Self::ControlUnknown(message),
        }
    }
}

pub type DeliveryOperation<'a, T = ()> =
    Pin<Box<dyn Future<Output = Result<T, DeliveryError>> + Send + 'a>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_errors_map_to_existing_control_errors() {
        assert!(matches!(
            Error::from(DeliveryError::Unsupported("unsupported".into())),
            Error::CapabilityUnavailable(message) if message == "unsupported"
        ));
        assert!(matches!(
            Error::from(DeliveryError::Offline("offline".into())),
            Error::CapabilityUnavailable(message) if message == "offline"
        ));
        assert!(matches!(
            Error::from(DeliveryError::StateChanged("changed".into())),
            Error::StateConflict(message) if message == "changed"
        ));
        assert!(matches!(
            Error::from(DeliveryError::Uncertain("uncertain".into())),
            Error::ControlUnknown(message) if message == "uncertain"
        ));
    }

    #[test]
    fn existing_control_errors_map_to_delivery_categories() {
        assert!(matches!(
            DeliveryError::from(Error::CapabilityUnavailable("offline".into())),
            DeliveryError::Offline(message) if message == "offline"
        ));
        assert!(matches!(
            DeliveryError::from(Error::StateConflict("changed".into())),
            DeliveryError::StateChanged(message) if message == "changed"
        ));
        assert!(matches!(
            DeliveryError::from(Error::ControlUnknown("uncertain".into())),
            DeliveryError::Uncertain(message) if message == "uncertain"
        ));
        assert!(matches!(
            DeliveryError::from(Error::Domain("offline".into())),
            DeliveryError::Offline(message) if message == "domain error: offline"
        ));
    }
}
