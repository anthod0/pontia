use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasoningEffortOption {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionModel {
    #[serde(default)]
    pub reasoning_efforts: Vec<ReasoningEffortOption>,
    pub default_reasoning_effort: Option<String>,
    pub id: String,
    pub name: String,
    pub description: String,
}
