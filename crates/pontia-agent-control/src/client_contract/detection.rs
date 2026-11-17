use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStatus {
    Detected,
    NotDetected,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDetection {
    pub status: DetectionStatus,
    pub detail: Option<String>,
}

impl ClientDetection {
    pub fn detected(detail: impl Into<String>) -> Self {
        Self {
            status: DetectionStatus::Detected,
            detail: Some(detail.into()),
        }
    }
    pub fn not_detected() -> Self {
        Self {
            status: DetectionStatus::NotDetected,
            detail: None,
        }
    }
    pub fn unknown(detail: impl Into<String>) -> Self {
        Self {
            status: DetectionStatus::Unknown,
            detail: Some(detail.into()),
        }
    }
}

/// The environment of the device performing detection, never the browser's host.
pub struct DetectionContext<'a> {
    pub vars: &'a HashMap<String, String>,
    pub pontia_home: &'a Path,
}

impl DetectionContext<'_> {
    pub fn command(&self, program: &str) -> ClientDetection {
        match pontia_runtime::executable::find(program, self.vars.get("PATH").map(String::as_str)) {
            Ok(Some(path)) => ClientDetection::detected(path.display().to_string()),
            Ok(None) => ClientDetection::not_detected(),
            Err(error) => ClientDetection::unknown(error),
        }
    }
}
