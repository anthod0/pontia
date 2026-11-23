use super::*;
use crate::{identity::BotRole, memory::MemoryAuthor};
use pontia_core::domain::SessionState;

fn memory(id: &str, content: &str) -> Memory {
    Memory {
        id: id.into(),
        kind: crate::memory::MemoryKind::Fact,
        content: content.into(),
        keywords: vec![],
        workspace_id: None,
        session_id: None,
        archived: false,
        organized: false,
        importance: crate::memory::Importance::Normal,
        occurred_at: None,
        sources: None,
        created_by: MemoryAuthor::Bot,
        created_at: "2026-11-21T12:54:17Z".into(),
        updated_at: "2026-11-21T12:54:17Z".into(),
        last_read_at: None,
    }
}

fn workspace(id: &str, name: &str, path: &str, description: Option<&str>) -> WorkspaceView {
    WorkspaceView {
        workspace_id: id.into(),
        canonical_path: path.into(),
        display_path: path.into(),
        name: Some(name.into()),
        description: description.map(Into::into),
        state: "active".into(),
        availability: pontia_agent_control::views::workspaces::WorkspaceAvailability::Available {
            root_id: "home".into(),
            path: name.into(),
        },
        metadata: serde_json::json!({}),
        created_at: "2026-11-21T12:00:00Z".into(),
        updated_at: "2026-11-21T12:00:00Z".into(),
        last_used_at: None,
    }
}

fn profile(description: &str) -> LaunchableAgentProfile {
    LaunchableAgentProfile {
        profile_id: "codex-default".into(),
        name: "Codex default model".into(),
        description: Some(description.into()),
        client_type: "codex".into(),
        client_key: "default".into(),
        model: "gpt-6-astra".into(),
        reasoning_effort: None,
    }
}

fn helper(id: &str, role: BotRole, description: &str) -> HelperContext {
    HelperContext {
        bot_id: "bot-example".into(),
        session_id: id.into(),
        role,
        workspace_id: Some("wks-project".into()),
        description: Some(description.into()),
        state: SessionState::Busy,
    }
}

fn message(id: &str, kind: MessageKind, content: &str, status: MessageStatus) -> HistoryMessage {
    HistoryMessage {
        related_to: vec![],
        message: crate::messages::BotMessage {
            id: id.into(),
            bot_id: "bot-example".into(),
            kind,
            content: content.into(),
            created_at: "2026-11-21T13:00:00Z".into(),
            status,
            todo: None,
        },
    }
}

#[test]
fn untrusted_markdown_stays_inside_its_record_or_field() {
    let malicious = "literal | `tick`\n# injected heading\n```\n- injected item\n\n</code><script>bad()</script>\r## another heading\r\n~~~\n```";
    let doc = document(
        "",
        None,
        &[workspace(
            "wks-project",
            malicious,
            "/tmp/`danger`\n# path",
            Some(malicious),
        )],
        "wks-temporary",
        &[pontia_config::WorkspaceRootConfig {
            label: "Home".into(),
            root_id: "`root`\n# root".into(),
            path: "/tmp/`path`\n# path".into(),
        }],
        &[memory("mem-example", malicious)],
        &[profile(malicious)],
        &[&helper("sess-example", BotRole::Helper, malicious)],
        &[message(
            "msg-example",
            MessageKind::UserInput,
            malicious,
            MessageStatus::Normal,
        )],
        &[],
    );
    let context = doc.split_once("# Handoff context\n").unwrap().1;
    assert!(!context.contains('\r'));
    assert_eq!(
        context
            .lines()
            .filter(|line| line.starts_with('#'))
            .collect::<Vec<_>>(),
        [
            "## User",
            "## Your Bot identity",
            "## Workspace",
            "### Workspace Roots",
            "## Memory",
            "## Agent Profiles",
            "## Helpers",
            "## Open todos",
            "## Recent Chat"
        ]
    );
    assert_eq!(
        context
            .lines()
            .filter(|line| line.starts_with("- "))
            .count(),
        6
    );
    assert_eq!(
        context
            .matches("      # injected heading\n      ```\n      - injected item\n      \n")
            .count(),
        2
    );
    assert_eq!(context.matches("      </code><script>bad()</script>\n      ## another heading\n      ~~~\n      ```\n").count(), 2);
    assert_eq!(
        context
            .matches("    \\# injected heading\n    \\`\\`\\`\n    \\- injected item\n    \n")
            .count(),
        4
    );
    assert!(context.contains("  - Path: ``/tmp/`danger` # path``\n"));
    assert!(context.contains("- `` `root` # root ``: ``/tmp/`path` # path``\n"));
}

#[test]
fn example_covers_project_temporary_retrieval_and_failed_delivery() {
    let doc = document(
        "Investigate before planning. Commit completed repository work locally without pushing.",
        Some("Coordinate project releases."),
        &[
            workspace(
                "wks-project",
                "pontia",
                "/home/example/projects/pontia",
                Some("Run and manage AI assistants on your own device."),
            ),
            workspace("wks-temporary", "Pontia", "/home/example/Pontia", None),
        ],
        "wks-temporary",
        &[pontia_config::WorkspaceRootConfig {
            label: "Home".into(),
            root_id: "home".into(),
            path: "/home/example".into(),
        }],
        &[
            memory(
                "mem-newer",
                "Multi-file cards are complete and committed locally.",
            ),
            memory(
                "mem-older",
                "The user prefers a prompt reply before investigation.",
            ),
        ],
        &[profile("Uses the model configured for Codex.")],
        &[
            &helper(
                "sess-helper",
                BotRole::Helper,
                "Investigate upgrades without interrupting agents",
            ),
            &helper(
                "sess-retrieval",
                BotRole::Retrieval,
                "Find last week's decisions about compaction",
            ),
        ],
        &[
            message(
                "msg-failed",
                MessageKind::BotReply,
                "This reply was not delivered.",
                MessageStatus::Failed,
            ),
            message(
                "msg-reply",
                MessageKind::BotReply,
                "The work is committed locally.",
                MessageStatus::Normal,
            ),
            message(
                "msg-user",
                MessageKind::UserInput,
                "Commit the work locally.",
                MessageStatus::Normal,
            ),
        ],
        &[],
    );
    let context = doc.split_once("# Handoff context\n").unwrap().1;
    assert!(context.contains("  - Type: Project\n"));
    assert!(context.contains("  - Type: Temporary\n"));
    let retrieval = context
        .split_once("- `sess-retrieval`\n")
        .unwrap()
        .1
        .split_once("## Recent Chat")
        .unwrap()
        .0;
    assert!(!retrieval.contains("Workspace ID"));
    assert!(context.contains("(`msg-failed`) [Delivery failed; history only]"));
    println!("{doc}");
}
