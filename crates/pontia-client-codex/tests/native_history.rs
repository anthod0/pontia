use pontia_agent_control::client_contract::{
    native_history::{HistorySource, NativeHistory},
    raw_transcripts::{AgentBindingResolveRequest, SessionTimelineSelection, SessionTimelineTurn},
};
use serde_json::{Value, json};
use std::{io::Write, path::Path, sync::Arc};

fn write(path: &Path, values: &[Value]) -> u64 {
    let mut file = std::fs::File::create(path).unwrap();
    for value in values {
        writeln!(file, "{value}").unwrap();
    }
    file.metadata().unwrap().len()
}
fn records(turn: &str) -> Vec<Value> {
    vec![
        json!({"type":"turn_context","payload":{"turn_id":turn}}),
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":turn}]}}),
        json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":turn}}),
    ]
}
fn head(base: Value) -> Value {
    json!({"type":"session_meta","payload":{"id":"thread","history_mode":"paginated","history_base":base}})
}
fn turn(id: &str) -> SessionTimelineTurn {
    SessionTimelineTurn {
        turn_id: format!("pontia-{id}"),
        head_cursor: Some(format!("codex-turn-v2:binding:thread:{id}:before")),
        tail_cursor: Some(format!("codex-turn-v2:binding:thread:{id}:after")),
    }
}
fn history(path: &Path) -> Arc<dyn NativeHistory> {
    pontia_client_codex::registration()
        .data
        .unwrap()
        .native_history(
            AgentBindingResolveRequest {
                id: "binding".into(),
                session_id: "session".into(),
                client_type: "codex".into(),
                client_session_key: "thread".into(),
                client_session_file: Some(path.into()),
            },
            None,
        )
        .unwrap()
}

#[tokio::test]
async fn file_preview_uses_current_sqlite_path_and_multilevel_compressed_prefixes() {
    let root = tempfile::tempdir().unwrap();
    let sessions = root.path().join("sessions");
    std::fs::create_dir(&sessions).unwrap();
    let f1 = sessions.join("rollout-F1.jsonl");
    let f2 = sessions.join("rollout-thread_F2.jsonl");
    let f3 = sessions.join("rollout-thread_F3.jsonl");
    let mut first = vec![head(Value::Null)];
    first.extend(records("A"));
    first.extend(records("B:colon"));
    let cutoff = write(&f1, &first);
    first.extend(records("C-removed"));
    write(&f1, &first);
    let compressed = zstd::stream::encode_all(std::fs::File::open(&f1).unwrap(), 1).unwrap();
    std::fs::write(sessions.join("rollout-F1.jsonl.zst"), compressed).unwrap();
    std::fs::remove_file(&f1).unwrap();
    let second = vec![head(
        json!({"thread_id":"F1","end_ordinal_exclusive":7,"end_byte_offset":cutoff}),
    )];
    let cutoff2 = write(&f2, &second);
    let mut second = second;
    second.extend(records("E-removed"));
    write(&f2, &second);
    let mut third = vec![head(
        json!({"thread_id":"F2","end_ordinal_exclusive":8,"end_byte_offset":cutoff2}),
    )];
    third.extend(records("F"));
    write(&f3, &third);
    let db = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(root.path().join("state_5.sqlite"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::query("CREATE TABLE threads(id TEXT,rollout_path TEXT)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO threads VALUES('thread',?)")
        .bind(f3.to_str().unwrap())
        .execute(&db)
        .await
        .unwrap();
    db.close().await;
    let reader = history(&f2); // The stored binding still points to the obsolete file.
    let mut turns = vec![
        turn("A"),
        turn("B:colon"),
        turn("C-removed"),
        turn("E-removed"),
        turn("F"),
    ];
    turns[0].head_cursor = Some(json!({"version":1,"binding":"binding","thread":"thread","native_turn":"A","offset":999999}).to_string());
    turns[0].tail_cursor = Some(
        json!({"version":1,"binding":"binding","native_turn":"A","offset":9999999}).to_string(),
    );
    let selection = reader
        .select_timeline(turns.clone(), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selection.source, HistorySource::File);
    assert_eq!(
        selection.turn_ids,
        ["pontia-A", "pontia-B:colon", "pontia-F"]
    );
    let items = reader
        .read_session_timeline(SessionTimelineSelection {
            turns,
            selected_turn_ids: selection.turn_ids,
        })
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| item.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        ["A", "B:colon", "F"]
    );
}

#[tokio::test]
async fn pinned_native_source_does_not_fall_back_and_file_failures_are_errors() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("rollout.jsonl");
    write(&file, &[head(Value::Null)]);
    assert!(
        history(&file)
            .select_timeline(vec![], Some(HistorySource::Native))
            .await
            .is_err()
    );
    let file_source = history(&file)
        .select_timeline(vec![], Some(HistorySource::File))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(file_source.source, HistorySource::File);
    std::fs::remove_file(&file).unwrap();
    assert!(
        history(&file)
            .select_timeline(vec![], Some(HistorySource::File))
            .await
            .is_err()
    );
}
