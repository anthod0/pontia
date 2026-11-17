use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Decision,
    Event,
    Lesson,
    Observation,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum Importance {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum MemoryAuthor {
    Bot,
    Dreaming,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Memory {
    pub id: String,
    pub kind: MemoryKind,
    pub content: String,
    #[sqlx(json)]
    pub keywords: Vec<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub archived: bool,
    pub importance: Importance,
    pub occurred_at: Option<String>,
    pub sources: Option<String>,
    pub created_by: MemoryAuthor,
    pub created_at: String,
    pub updated_at: String,
    pub last_read_at: Option<String>,
}

/// Roles and scope come from trusted Session bindings, never from command input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryRole {
    Bot,
    Retrieval,
    Dreaming,
}

#[derive(Clone, Debug)]
pub struct MemoryAccess {
    pub role: MemoryRole,
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMemory {
    pub kind: MemoryKind,
    pub content: String,
    pub keywords: Vec<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    #[serde(default)]
    pub importance: Importance,
    pub occurred_at: Option<String>,
    pub sources: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMemory {
    pub id: String,
    pub kind: MemoryKind,
    pub content: String,
    pub keywords: Vec<String>,
    pub importance: Importance,
    pub occurred_at: Option<String>,
    pub sources: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveMemory {
    pub id: String,
    pub archived: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEdge {
    pub memory_a_id: String,
    pub memory_b_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadMemories {
    pub ids: Vec<String>,
    pub workspace_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchMemories {
    pub keywords: Vec<String>,
    pub workspace_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryNeighbors {
    pub id: String,
    pub workspace_id: Option<String>,
    pub after: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "action",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MemoryCommand {
    Create(CreateMemory),
    Update(UpdateMemory),
    Archive(ArchiveMemory),
    Link(MemoryEdge),
    Unlink(MemoryEdge),
    Read(ReadMemories),
    Search(SearchMemories),
    Neighbors(MemoryNeighbors),
    Use(ReadMemories),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequest {
    pub session_id: String,
    pub command: MemoryCommand,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryHit {
    pub memory: Memory,
    pub relevance: f64,
    pub score: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum MemoryResponse {
    Created {
        memory: Memory,
    },
    Changed {
        affected: u64,
    },
    Read {
        memories: Vec<Memory>,
    },
    Search {
        results: Vec<MemoryHit>,
        has_more: bool,
    },
    Neighbors {
        memories: Vec<Memory>,
        next_cursor: Option<String>,
    },
}
