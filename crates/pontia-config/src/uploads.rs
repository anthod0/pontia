use std::num::NonZeroU32;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UploadsConfig {
    pub incomplete_retention_days: NonZeroU32,
    pub completed_retention_days: NonZeroU32,
    pub cleanup_interval_hours: NonZeroU32,
}

impl Default for UploadsConfig {
    fn default() -> Self {
        Self {
            incomplete_retention_days: NonZeroU32::new(7).unwrap(),
            completed_retention_days: NonZeroU32::new(30).unwrap(),
            cleanup_interval_hours: NonZeroU32::new(24).unwrap(),
        }
    }
}
