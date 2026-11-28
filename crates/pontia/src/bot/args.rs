use clap::{Args, Subcommand};
use serde::Serialize;

#[derive(Debug, Args)]
pub(crate) struct BotCommand {
    /// Print the result as JSON instead of plain text
    #[arg(long, global = true)]
    pub(super) json: bool,
    #[command(subcommand)]
    pub(super) command: BotTool,
}

#[derive(Debug, Subcommand)]
pub(super) enum BotTool {
    /// Ask a Bot to clarify ambiguous memories (dreaming)
    #[command(subcommand)]
    Dreaming(DreamingCommand),
    /// Create, message, inspect, compact, or close helpers; helpers use reply to report to their Bot
    #[command(subcommand)]
    Helper(HelperCommand),
    /// Search or inspect this Bot's own chat history
    ChatHistory(ChatHistory),
    /// Read Session messages without changing execution state
    SessionMessages(SessionMessages),
    /// Update shared USER.md (at most 2,000 characters)
    UpdateUser(UpdateUser),
    /// Set this Bot's description while it is empty (at most 2,000 characters)
    SetDescription {
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
        description: String,
    },
    /// Replace the links from an unquoted user message to earlier messages in this chat
    LinkMessage {
        #[arg(long, value_name = "ID")]
        message_id: String,
        /// Earlier message IDs (one to five distinct targets)
        #[arg(long, value_name = "ID", required = true, num_args = 1..)]
        related_to: Vec<String>,
    },
    /// Update a todo on an existing chat message
    #[command(subcommand)]
    Todo(TodoCommand),
    /// Send one complete reply to the user
    SendToUser(SendToUser),
    /// Manage shared Agent Profiles
    #[command(subcommand, after_help = super::profile::CREATE_HELP)]
    Profile(super::profile::ProfileCommand),
    /// Register an existing directory or change a Workspace description
    #[command(subcommand)]
    Workspace(WorkspaceCommand),
    /// Read or maintain device memories
    Memory(MemoryArgs),
}

#[derive(Debug, Args)]
#[command(
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true,
    after_help = "Examples:\n  pontia bot chat-history --query 'deployment failed'\n  pontia bot chat-history context --message-id MESSAGE_ID --radius 3"
)]
pub(super) struct ChatHistory {
    /// Literal text to search for, newest matches first
    #[arg(long, required = true, allow_hyphen_values = true)]
    query: Option<String>,
    /// Match message text, files, or images (default: all)
    #[arg(long = "type", value_parser = ["all", "file", "image"])]
    kind: Option<String>,
    /// Continue from next_before in the preceding search page
    #[arg(long)]
    before: Option<String>,
    /// Maximum search results, from 1 to 100 (default: 10)
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
    limit: Option<u32>,
    #[command(subcommand)]
    command: Option<ChatHistoryCommand>,
}

#[derive(Debug, Subcommand)]
pub(super) enum ChatHistoryCommand {
    /// Search for literal text, newest matches first
    Search {
        #[arg(long, allow_hyphen_values = true)]
        keyword: String,
        /// Continue from next_before in the preceding page
        #[arg(long)]
        before: Option<String>,
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
    },
    /// Read the matched message and its chronological neighbors
    Context {
        #[arg(long)]
        message_id: String,
        /// Maximum messages on each side of the anchor
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(0..=50))]
        radius: u32,
    },
}

impl ChatHistory {
    pub(super) fn into_query(self) -> pontia_bot::conversation::chat_history::ChatQuery {
        match self.command {
            Some(ChatHistoryCommand::Search {
                keyword,
                before,
                limit,
            }) => pontia_bot::conversation::chat_history::ChatQuery::Search {
                query: keyword,
                kind: pontia_bot::messages::SearchType::All,
                before,
                limit,
            },
            Some(ChatHistoryCommand::Context { message_id, radius }) => {
                pontia_bot::conversation::chat_history::ChatQuery::Context { message_id, radius }
            }
            None => pontia_bot::conversation::chat_history::ChatQuery::Search {
                query: self.query.expect("clap requires a chat history query"),
                kind: match self.kind.as_deref() {
                    None | Some("all") => pontia_bot::messages::SearchType::All,
                    Some("file") => pontia_bot::messages::SearchType::File,
                    Some("image") => pontia_bot::messages::SearchType::Image,
                    Some(_) => unreachable!("clap validates chat history search types"),
                },
                before: self.before,
                limit: self.limit.unwrap_or(10),
            },
        }
    }
}

#[derive(Debug, Subcommand, Serialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub(super) enum TodoCommand {
    Mark {
        #[arg(long)]
        message_id: String,
    },
    Resolve {
        #[arg(long)]
        message_id: String,
    },
}

#[derive(Debug, Args, Serialize)]
#[command(after_help = "Example: pontia bot send-to-user --message 'I received your request.'")]
pub(super) struct SendToUser {
    /// Complete user-visible reply
    #[arg(long, allow_hyphen_values = true)]
    message: String,
    /// Earlier message in this Bot chat that this reply follows (repeat up to five times)
    #[arg(long = "related-to", value_name = "ID")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    related_to: Vec<String>,
    /// Mark this reply as an open todo for the user
    #[arg(long)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    todo: bool,
    /// Close an open todo in this Bot chat with this reply (repeatable)
    #[arg(long = "resolve-todo", value_name = "ID")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    resolve_todos: Vec<String>,
}

#[derive(Debug, Args, Serialize)]
#[command(
    after_help = "Pages are chronological. Tools are excluded by default; repeat --category to select categories.\nUse the continuation command in the result to read the next page, even when no messages matched.\nSession execution status is queried separately."
)]
pub(super) struct SessionMessages {
    /// Target Session, distinct from the caller in PONTIA_SESSION_ID
    #[arg(long)]
    session_id: String,
    /// Inclusive Turn anchor; omit to start from the newest/oldest Turn
    #[arg(long)]
    turn_id: Option<String>,
    #[arg(long, default_value = "backward", value_parser = ["backward", "forward"])]
    direction: String,
    /// Number of Turns scanned per page (1–100)
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=100))]
    limit: u32,
    /// Message category; repeat to select more than one
    #[arg(long = "category", default_values = ["user", "assistant", "other"], value_parser = ["user", "assistant", "other", "tool_call", "tool_result"])]
    categories: Vec<String>,
    /// Keep the history source returned by the previous page
    #[arg(long, value_parser = ["native", "file"])]
    history_source: Option<String>,
}

#[derive(Debug, Args, Serialize)]
pub(super) struct UpdateUser {
    /// Text to replace exactly once; an empty value appends
    #[arg(long, allow_hyphen_values = true)]
    old_text: String,
    /// Replacement text; an empty value deletes the match
    #[arg(long, allow_hyphen_values = true)]
    new_text: String,
}

#[derive(Debug, Subcommand, Serialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub(super) enum WorkspaceCommand {
    /// Register and activate an existing directory within a configured root
    Register {
        #[arg(long)]
        root_id: String,
        /// Path relative to the root; omit to register the root itself
        #[arg(long, default_value = "")]
        path: String,
        #[arg(long)]
        name: Option<String>,
        /// Description, at most 2,000 characters; omission preserves existing text
        #[arg(long, allow_hyphen_values = true)]
        description: Option<String>,
    },
    /// Set or clear the description without changing activation state
    Describe {
        #[arg(long)]
        workspace_id: String,
        #[arg(
            long,
            required_unless_present = "clear_description",
            conflicts_with = "clear_description",
            allow_hyphen_values = true
        )]
        description: Option<String>,
        /// Clear the description
        #[arg(long)]
        #[serde(skip)]
        clear_description: bool,
    },
}

#[derive(Debug, Args, Serialize)]
pub(super) struct MemoryContent {
    #[arg(long, value_parser = ["fact", "decision", "event", "lesson", "observation"])]
    kind: String,
    /// Complete memory text
    #[arg(long, allow_hyphen_values = true)]
    content: String,
    /// Whole keyword; repeat for 1–32 keywords (1–128 characters each)
    #[arg(long = "keyword", required = true)]
    keywords: Vec<String>,
    /// RFC 3339 event time
    #[arg(long)]
    occurred_at: Option<String>,
    /// Natural-language source description
    #[arg(long, allow_hyphen_values = true)]
    sources: Option<String>,
}

#[derive(Debug, Args, Serialize)]
pub(super) struct MemoryIds {
    /// Memory ID; repeat for up to 100 IDs
    #[arg(long = "id", required = true)]
    ids: Vec<String>,
    /// Restrict the operation to this Workspace
    #[arg(long)]
    workspace_id: Option<String>,
}

#[derive(Debug, Args, Serialize)]
pub(super) struct MemoryEdge {
    #[arg(long)]
    memory_a_id: String,
    #[arg(long)]
    memory_b_id: String,
}

#[derive(Debug, Subcommand, Serialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub(super) enum MemoryCommand {
    /// List unarchived memories still awaiting organization (dreaming)
    Pending {
        #[arg(long)]
        after: Option<String>,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
    },
    /// Mark organization complete, or leave a memory pending (dreaming)
    Organize {
        #[arg(long)]
        id: String,
        #[arg(long, required = true, action = clap::ArgAction::Set)]
        organized: bool,
    },
    /// Create a memory (Bot or dreaming)
    Create {
        #[command(flatten)]
        #[serde(flatten)]
        content: MemoryContent,
        #[arg(long, default_value = "normal", value_parser = ["low", "normal", "high"])]
        importance: String,
        #[arg(long)]
        workspace_id: Option<String>,
        /// Session associated with this memory, not the caller identity
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Replace memory content (dreaming); omitted sources and event time are cleared
    Update {
        #[arg(long)]
        id: String,
        #[command(flatten)]
        #[serde(flatten)]
        content: MemoryContent,
        #[arg(long, value_parser = ["low", "normal", "high"])]
        importance: String,
    },
    /// Archive or restore a memory (dreaming)
    Archive {
        #[arg(long)]
        id: String,
        /// true archives the memory; false restores it
        #[arg(long, required = true, action = clap::ArgAction::Set)]
        archived: bool,
    },
    /// Link two active memories (dreaming)
    Link(MemoryEdge),
    /// Remove a link between memories (dreaming)
    Unlink(MemoryEdge),
    /// Read active memories by ID without updating their usage time
    Read(MemoryIds),
    /// Search whole keywords; body text is not searched
    Search {
        /// Repeat for 1–32 keywords
        #[arg(long = "keyword", required = true)]
        keywords: Vec<String>,
        #[arg(long)]
        workspace_id: Option<String>,
        /// Maximum results (1–30); has_more refers to the top 100 candidates
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=30))]
        limit: u32,
    },
    /// Start read-only deep retrieval and receive an asynchronous helper reply (Bot)
    Retrieve {
        /// Question and context for the investigation
        #[arg(long, allow_hyphen_values = true)]
        message: String,
        /// Ask the helper to focus on this Workspace; does not restrict its read access
        #[arg(long)]
        workspace_id: Option<String>,
    },
    /// Read one hop of linked active memories
    Neighbors {
        #[arg(long)]
        id: String,
        #[arg(long)]
        workspace_id: Option<String>,
        /// Cursor returned by the preceding page
        #[arg(long)]
        after: Option<String>,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
    },
    /// Record memories actually used in a response or decision (Bot)
    Use(MemoryIds),
}

#[derive(Debug, Subcommand, Serialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub(super) enum HelperCommand {
    /// Start a helper using an Agent Profile, or the general helper with this Bot's client and model
    Create {
        /// Start in this active project Workspace
        #[arg(
            long,
            required_unless_present = "temporary",
            conflicts_with = "temporary"
        )]
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace_id: Option<String>,
        /// Keep this task's files in ~/Pontia/NAME
        #[arg(long, value_name = "NAME")]
        #[serde(skip_serializing_if = "Option::is_none")]
        temporary: Option<String>,
        #[arg(long)]
        profile_id: Option<String>,
        /// Describe what the helper is doing; shown as the Bot work status
        #[arg(long, allow_hyphen_values = true)]
        title: String,
        #[arg(long, allow_hyphen_values = true)]
        description: String,
        #[arg(long, allow_hyphen_values = true)]
        message: String,
    },
    /// List associated Sessions that have not exited, including idle helpers
    List,
    /// Send additional instructions to one of your helpers
    Send {
        #[arg(long)]
        session_id: String,
        #[arg(long, allow_hyphen_values = true)]
        message: String,
    },
    /// Report to your Bot's current Session (helper only)
    Reply {
        #[arg(long, allow_hyphen_values = true)]
        message: String,
    },
    /// Read status and paginated messages without affecting execution
    Check(SessionMessages),
    /// Start context compaction for one of your idle or interrupted helpers
    #[command(
        after_help = "Requires a client with manual compaction available. Wait for a working helper to become idle.\nAn accepted result and operation ID mean compaction was requested, not completed."
    )]
    Compact {
        #[arg(long)]
        session_id: String,
    },
    /// Exit a helper Session and release its runtime resources, retaining history
    Close {
        #[arg(long)]
        session_id: String,
    },
}

#[derive(Debug, Subcommand, Serialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub(super) enum DreamingCommand {
    Clarify {
        #[arg(long)]
        bot_id: String,
        #[arg(long, allow_hyphen_values = true)]
        message: String,
    },
}

#[derive(Debug, Args)]
pub(super) struct MemoryArgs {
    /// Summary and evidence for a change (required for dreaming writes)
    #[arg(long, global = true, allow_hyphen_values = true)]
    pub reason: Option<String>,
    #[command(subcommand)]
    pub command: MemoryCommand,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn chat_history_commands_parse_only_scoped_reads() {
        for args in [
            vec!["--query", "部署", "--type", "file", "--limit", "2"],
            vec!["search", "--keyword", "部署"],
            vec!["context", "--message-id", "m1", "--radius", "0"],
        ] {
            let cli = crate::Cli::try_parse_from(
                ["pontia", "bot", "chat-history"].into_iter().chain(args),
            )
            .unwrap();
            let crate::Command::Bot(BotCommand {
                command: BotTool::ChatHistory(input),
                ..
            }) = cli.command
            else {
                panic!("expected chat-history")
            };
            let _ = input.into_query();
        }
        for args in [
            vec!["--query", "部署", "--bot-id", "other"],
            vec!["context", "--message-id", "m1", "--radius", "51"],
            vec!["--query", "部署", "context", "--message-id", "m1"],
            vec!["--type", "file"],
            vec!["delete", "--message-id", "m1"],
        ] {
            assert!(
                crate::Cli::try_parse_from(
                    ["pontia", "bot", "chat-history"]
                        .into_iter()
                        .chain(args.clone())
                )
                .is_err(),
                "{args:?}"
            );
        }
    }
}
