use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphRuntimeConfig {
    pub enabled: bool,
    pub db_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceNode {
    pub id: String,
    pub kind: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskProvenance {
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}
