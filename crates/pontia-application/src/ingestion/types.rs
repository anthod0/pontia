#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventIngestResult {
    pub accepted: bool,
    pub duplicate: bool,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub state_version: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum EventReportError {
    #[error("{0}")]
    InvalidFact(String),
    #[error(transparent)]
    Ingestion(#[from] pontia_core::Error),
}

impl EventReportError {
    pub fn is_permanent_rejection(&self) -> bool {
        matches!(
            self,
            Self::InvalidFact(_)
                | Self::Ingestion(
                    pontia_core::Error::Domain(_)
                        | pontia_core::Error::StateConflict(_)
                        | pontia_core::Error::NotFound(_)
                        | pontia_core::Error::CapabilityUnavailable(_)
                )
        )
    }

    pub(super) fn validation(error: pontia_core::Error) -> Self {
        match error {
            pontia_core::Error::Domain(message) => Self::InvalidFact(message),
            other => Self::Ingestion(other),
        }
    }
}
