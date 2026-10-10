//! Test peer owns the fixture history. The application only receives RPC pages.
use super::*;
use pontia_client_pi::rpc::PiRpcPeer;
use tokio::net::UnixStream;

pub(super) async fn attach_fixture(
    state: &AppState,
    session: &str,
    native: &str,
    fixture: PathBuf,
) -> Arc<PiRpcPeer> {
    attach(state, session, native, move || {
        let contents = fs::read_to_string(&fixture).unwrap_or_default();
        contents
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|entry| entry["type"] != "session")
            .map(|mut entry| {
                if entry.get("timestamp").is_none() {
                    entry["timestamp"] = json!("2026-10-11T00:00:00Z");
                }
                entry
            })
            .collect()
    })
    .await
}
pub(super) async fn attach(
    state: &AppState,
    session: &str,
    native: &str,
    entries: impl Fn() -> Vec<Value> + Send + 'static,
) -> Arc<PiRpcPeer> {
    let runtime: String = sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=? ORDER BY created_at DESC LIMIT 1").bind(session).fetch_one(&state.db()).await.unwrap();
    let (left, right) = UnixStream::pair().unwrap();
    let (daemon, _) = PiRpcPeer::new(left);
    let (peer, mut requests) = PiRpcPeer::new(right);
    state
        .client_control()
        .attach("pi", session, &runtime, native, daemon.clone())
        .await
        .unwrap();
    let native = native.to_owned();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            if request.method == "history.read" {
                let entries = entries();
                let start = request.params["continuation"]
                    .as_str()
                    .map_or(0, |s| s.parse::<usize>().unwrap());
                let end = (start + 128).min(entries.len());
                let upper = entries.last().map(|e| e["id"].clone());
                peer.reply(request.id,json!({"session_id":native,"snapshot":"fixture","entry_count":entries.len(),"upper_entry_id":upper,"leaf_id":upper,"entries":entries[start..end],"continuation":if end<entries.len(){Some(end.to_string())}else{None}})).await.unwrap();
            } else if request.method == "ping" {
                peer.reply(request.id, json!({"pong":true})).await.unwrap();
            } else {
                peer.reply_error(request.id, -32601, "unsupported")
                    .await
                    .unwrap();
            }
        }
    });
    daemon
}

#[tokio::test]
async fn http_timeline_and_tree_use_runtime_history_when_the_bound_file_does_not_exist() {
    let root = tempdir().unwrap();
    let state = test_state().await;
    let session = "sess_rpc_only";
    let native = "native-rpc-only";
    seed_session(&state, session).await;
    let missing = root.path().join("missing.jsonl");
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session.into(),
            client_type: "pi".into(),
            launch_cwd: root.path().display().to_string(),
            client_session_key: native.into(),
            client_session_file: Some(missing.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    post_pi_turn_event(
        state.clone(),
        session,
        "turn_rpc",
        "unused-start",
        "turn.started",
        json!({"previous_leaf_id":null}),
    )
    .await;
    post_pi_turn_event(
        state.clone(),
        session,
        "turn_rpc",
        "unused-end",
        "turn.completed",
        json!({"terminal_leaf_id":"a"}),
    )
    .await;
    let turn = state
        .event_ingest_service()
        .get_turn("turn_rpc")
        .await
        .unwrap()
        .unwrap();
    let cursor = pontia_client_pi::history::PiEntryCursor::decode(
        turn.head_cursor.as_deref().unwrap(),
        &binding.id,
        Some(native),
    )
    .unwrap();
    assert!(cursor.anchor.is_none());
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let daemon = attach(&state,session,native,move || { count.fetch_add(1,std::sync::atomic::Ordering::SeqCst); vec![json!({"id":"u","parentId":null,"type":"message","timestamp":"2026-10-11T00:00:00Z","message":{"role":"user","content":"question"}}),json!({"id":"a","parentId":"u","type":"message","timestamp":"2026-10-11T00:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}})] }).await;
    let (status, body) = get_json(
        state.clone(),
        &format!("/api/v1/sessions/{session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["items"][1]["content_preview"], "answer");
    // Recovery and timeline mapping must share the first request's snapshot.
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    for endpoint in ["history", "updates"] {
        let (status, body) = get_json(
            state.clone(),
            &format!("/api/v1/sessions/{session}/turns/tree/{endpoint}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(
            body["data"]["groups"][0]["items"][0]["content_preview"],
            "question"
        );
    }
    assert!(!missing.exists());
    daemon.close();
    let (status, body) = get_json(
        state,
        &format!("/api/v1/sessions/{session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "timeline_source_unavailable");
    assert!(
        !body
            .to_string()
            .contains(&root.path().display().to_string())
    );
}

#[tokio::test]
async fn terminal_capture_preserves_an_existing_legacy_turn_then_new_turns_use_entry_locators() {
    let root = tempdir().unwrap();
    let state = test_state().await;
    let session = "sess_upgrade";
    seed_session(&state, session).await;
    let path = root.path().join("legacy.jsonl");
    fs::write(&path, b"").unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session.into(),
            client_type: "pi".into(),
            launch_cwd: root.path().display().to_string(),
            client_session_key: "native-upgrade".into(),
            client_session_file: Some(path.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    post_pi_turn_event(
        state.clone(),
        session,
        "turn_old",
        "unused-start",
        "turn.started",
        json!({"previous_leaf_id":null}),
    )
    .await;
    let head = PiJsonlV2Cursor {
        binding_id: binding.id.clone(),
        byte_offset: 0,
        native_entry_anchor: None,
        relation: TimelineBoundaryRelation::After,
    }
    .encode();
    // This is the persisted active Turn shape present before upgrading.
    sqlx::query("UPDATE turns SET head_cursor=? WHERE turn_id='turn_old'")
        .bind(&head)
        .execute(&state.db())
        .await
        .unwrap();
    fs::write(&path,concat!("{\"id\":\"old-u\",\"parentId\":null,\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":\"old\"}}\n","{\"id\":\"old-a\",\"parentId\":\"old-u\",\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n")).unwrap();
    post_pi_turn_event(
        state.clone(),
        session,
        "turn_old",
        "unused-end",
        "turn.completed",
        json!({"terminal_leaf_id":"old-a"}),
    )
    .await;
    let old = state
        .event_ingest_service()
        .get_turn("turn_old")
        .await
        .unwrap()
        .unwrap();
    let tail = PiJsonlV2Cursor::decode(old.tail_cursor.as_deref().unwrap(), &binding.id).unwrap();
    assert_eq!(
        tail.byte_offset,
        fs::metadata(&path).unwrap().len() as usize
    );
    assert_eq!(tail.native_entry_anchor.as_deref(), Some("old-a"));
    let (status, body) = get_json(
        state.clone(),
        &format!("/api/v1/sessions/{session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);
    post_pi_turn_event(
        state.clone(),
        session,
        "turn_new",
        "unused-new",
        "turn.started",
        json!({"previous_leaf_id":"old-a"}),
    )
    .await;
    let new = state
        .event_ingest_service()
        .get_turn("turn_new")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pontia_client_pi::history::PiEntryCursor::decode(
            new.head_cursor.as_deref().unwrap(),
            &binding.id,
            Some("native-upgrade")
        )
        .unwrap()
        .anchor
        .as_deref(),
        Some("old-a")
    );
}
