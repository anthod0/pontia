use pontia_application::client_contract::{
    TopologyResolution,
    history::{HistoryRecoveryRequest, TurnHistoryCandidate},
    raw_transcripts::{
        ResolvedAgentBinding, TurnTimelineRange, TurnTimelineReadRequest, TurnTimelineReader,
    },
};
use pontia_client_pi::raw_transcripts::PiTimelineAdapter;
use pontia_core::domain::{TurnState, TurnTopology};
use serde_json::json;

fn entry(id: &str, parent: Option<&str>, role: &str) -> String {
    format!(
        "{}\n",
        json!({"id": id, "parentId": parent, "type": "message", "message": {"role": role, "content": [{"type":"text", "text":id}]}})
    )
}

fn cursor(offset: usize, anchor: &str) -> String {
    format!("pi-jsonl-v2:binding:{offset}:after:{anchor}")
}

fn turn(id: &str, head: String, tail: Option<String>, state: TurnState) -> TurnHistoryCandidate {
    TurnHistoryCandidate {
        turn_id: id.into(),
        head_cursor: Some(head),
        tail_cursor: tail,
        state,
        topology: TurnTopology::Unknown,
    }
}

#[test]
fn first_turn_recovery_treats_the_session_header_as_file_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("native.jsonl");
    let contents = format!(
        "{}\n",
        json!({"type":"session", "version":3, "id":"native"})
    ) + &entry("first-user", None, "user")
        + &entry("tool-call", Some("first-user"), "assistant");
    std::fs::write(&path, &contents).unwrap();
    let source = ResolvedAgentBinding {
        id: "binding".into(),
        client_type: "pi".into(),
        format: "pi-jsonl".into(),
        path,
        fingerprint: None,
    };
    let recovered = pontia_client_pi::registration(None)
        .data
        .unwrap()
        .history_recovery()
        .unwrap()
        .recover(HistoryRecoveryRequest {
            source: source.clone(),
            turns: vec![
                turn("turn1", cursor(0, ""), None, TurnState::Abandoned),
                turn(
                    "turn2",
                    cursor(contents.len(), "tool-call"),
                    None,
                    TurnState::Running,
                ),
            ],
        })
        .unwrap();
    assert_eq!(recovered[0].topology.resolution, TopologyResolution::Root);
    assert_eq!(
        recovered[1].topology.resolution,
        TopologyResolution::Linked {
            parent_turn_id: "turn1".into()
        }
    );
    let items = PiTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source,
            ranges: vec![TurnTimelineRange {
                turn_id: "turn1".into(),
                is_first_session_turn: true,
                head_cursor: cursor(0, ""),
                tail_cursor: recovered[0].tail_cursor.clone(),
            }],
        })
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["first-user", "tool-call"]
    );
}

#[test]
fn recovers_a_sealed_crash_range_and_links_the_native_branch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("native.jsonl");
    let before = entry("user1", None, "user") + &entry("answer1", Some("user1"), "assistant");
    let crash =
        entry("user2", Some("answer1"), "user") + &entry("tool_call", Some("user2"), "assistant");
    let end = before.len() + crash.len();
    let after =
        entry("user3", Some("tool_call"), "user") + &entry("answer3", Some("user3"), "assistant");
    let contents = before.clone() + &crash + &after;
    std::fs::write(&path, &contents).unwrap();
    let source = ResolvedAgentBinding {
        id: "binding".into(),
        client_type: "pi".into(),
        format: "pi-jsonl".into(),
        path,
        fingerprint: None,
    };
    let turns = vec![
        turn(
            "turn1",
            cursor(0, ""),
            Some(cursor(before.len(), "answer1")),
            TurnState::Completed,
        ),
        turn(
            "turn2",
            cursor(before.len(), "answer1"),
            None,
            TurnState::Abandoned,
        ),
        turn(
            "turn3",
            cursor(end, "tool_call"),
            Some(cursor(contents.len(), "answer3")),
            TurnState::Completed,
        ),
    ];
    let data = pontia_client_pi::registration(None).data.unwrap();
    let recovered = data
        .history_recovery()
        .unwrap()
        .recover(HistoryRecoveryRequest {
            source: source.clone(),
            turns,
        })
        .unwrap();
    assert_eq!(recovered[1].tail_cursor, Some(cursor(end, "tool_call")));
    assert_eq!(
        recovered[2].topology.resolution,
        TopologyResolution::Linked {
            parent_turn_id: "turn2".into()
        }
    );
    let items = PiTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source,
            ranges: vec![TurnTimelineRange {
                turn_id: "turn2".into(),
                is_first_session_turn: false,
                head_cursor: cursor(before.len(), "answer1"),
                tail_cursor: recovered[1].tail_cursor.clone(),
            }],
        })
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["user2", "tool_call"]
    );
}

#[test]
fn a_resume_on_an_older_branch_does_not_link_to_the_crashed_turn() {
    let root = tempfile::tempdir().unwrap();
    let before = entry("user1", None, "user") + &entry("answer1", Some("user1"), "assistant");
    let crash =
        entry("user2", Some("answer1"), "user") + &entry("tool_call", Some("user2"), "assistant");
    let end = before.len() + crash.len();
    let contents = before.clone() + &crash + &entry("edited_user", Some("answer1"), "user");
    let path = root.path().join("native.jsonl");
    std::fs::write(&path, contents).unwrap();
    let recovered = pontia_client_pi::registration(None)
        .data
        .unwrap()
        .history_recovery()
        .unwrap()
        .recover(HistoryRecoveryRequest {
            source: ResolvedAgentBinding {
                id: "binding".into(),
                client_type: "pi".into(),
                format: "pi-jsonl".into(),
                path,
                fingerprint: None,
            },
            turns: vec![
                turn(
                    "turn1",
                    cursor(0, ""),
                    Some(cursor(before.len(), "answer1")),
                    TurnState::Completed,
                ),
                turn(
                    "turn2",
                    cursor(before.len(), "answer1"),
                    None,
                    TurnState::Abandoned,
                ),
                turn("turn3", cursor(end, "answer1"), None, TurnState::Running),
            ],
        })
        .unwrap();
    assert_eq!(recovered[1].tail_cursor, Some(cursor(end, "tool_call")));
    assert_eq!(
        recovered[2].topology.resolution,
        TopologyResolution::Linked {
            parent_turn_id: "turn1".into()
        }
    );
}

#[test]
fn ambiguous_or_unsealed_crash_ranges_remain_unresolved() {
    let root = tempfile::tempdir().unwrap();
    let before = entry("user1", None, "user") + &entry("answer1", Some("user1"), "assistant");
    for (crash, seal, anchor) in [
        (entry("user2", Some("answer1"), "user"), false, "user2"),
        (
            entry("user2", Some("answer1"), "user") + &entry("untracked", Some("user2"), "user"),
            true,
            "untracked",
        ),
        (
            entry("user2", Some("answer1"), "user")
                + &entry("sibling", Some("answer1"), "assistant"),
            true,
            "sibling",
        ),
        (entry("user2", Some("missing"), "user"), true, "user2"),
    ] {
        let end = before.len() + crash.len();
        let path = root.path().join("native.jsonl");
        std::fs::write(&path, before.clone() + &crash).unwrap();
        let mut turns = vec![
            turn(
                "turn1",
                cursor(0, ""),
                Some(cursor(before.len(), "answer1")),
                TurnState::Completed,
            ),
            turn(
                "turn2",
                cursor(before.len(), "answer1"),
                None,
                TurnState::Abandoned,
            ),
        ];
        if seal {
            turns.push(turn("turn3", cursor(end, anchor), None, TurnState::Running));
        }
        let recovered = pontia_client_pi::registration(None)
            .data
            .unwrap()
            .history_recovery()
            .unwrap()
            .recover(HistoryRecoveryRequest {
                source: ResolvedAgentBinding {
                    id: "binding".into(),
                    client_type: "pi".into(),
                    format: "pi-jsonl".into(),
                    path,
                    fingerprint: None,
                },
                turns,
            })
            .unwrap();
        assert!(recovered[1].tail_cursor.is_none());
    }
}
