mod support;
use pontia_application::{
    AgentBindingService, AppState, PontiaEvent, PontiaEventSource, PontiaEventType,
    UpsertAgentBindingRequest,
    client_contract::{
        history::TurnHistoryCandidate,
        raw_transcripts::{AgentBindingResolveRequest, TurnTimelineRange, TurnTimelineReadError},
    },
};
use pontia_client_pi::{
    history::PiEntryCursor,
    rpc::{PiRpcPeer, RpcRequest},
};
use pontia_core::domain::{TurnState, TurnTopology};
use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::session_runtimes::{SessionRuntimeRecord, SqliteSessionRuntimeRepository},
    run_migrations,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{net::UnixStream, sync::mpsc};

struct Fixture {
    state: AppState,
    binding: AgentBindingResolveRequest,
    _root: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let db = connect_sqlite("sqlite::memory:").await.unwrap();
        run_migrations(&db).await.unwrap();
        let state = AppState::builder(db, root.path().into())
            .clients(support::clients())
            .build();
        state
            .event_ingest_service()
            .ingest_pontia_event(PontiaEvent::new(
                "session",
                None,
                PontiaEventSource::ExternalApi,
                "pi",
                PontiaEventType::SessionCreated,
                json!({}),
            ))
            .await
            .unwrap();
        SqliteSessionRuntimeRepository::new(state.db())
            .upsert_binding(SessionRuntimeRecord {
                session_id: "session".into(),
                runtime_id: "runtime".into(),
                start_command: None,
                tmux_socket_path: None,
                tmux_pane_id: None,
                process_fingerprint: None,
                role: "tui".into(),
                state: "running".into(),
                created_at: "2026-10-11T00:00:00Z".into(),
            })
            .await
            .unwrap();
        let binding = AgentBindingService::new(state.db())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: "session".into(),
                client_type: "pi".into(),
                launch_cwd: root.path().display().to_string(),
                client_session_key: "native".into(),
                client_session_file: Some(
                    root.path()
                        .join("does-not-exist.jsonl")
                        .display()
                        .to_string(),
                ),
                metadata: json!({}),
            })
            .await
            .unwrap();
        state.event_ingest_service().report_fact(pontia_application::ReportedFact { session_id: "session".into(), turn_id: None, fact_type: pontia_core::domain::EventType::SessionReady, data: json!({"runtime_id":"runtime", "client_session_key":"native", "client_cwd":root.path(), "client_session_file":root.path().join("does-not-exist.jsonl")}) }).await.unwrap();
        Self {
            binding: AgentBindingResolveRequest {
                id: binding.id,
                session_id: "session".into(),
                client_type: "pi".into(),
                client_session_key: "native".into(),
                client_session_file: binding.client_session_file.map(Into::into),
            },
            state,
            _root: root,
        }
    }
    fn cursor(&self, anchor: Option<&str>) -> String {
        PiEntryCursor {
            binding_id: self.binding.id.clone(),
            session_id: "native".into(),
            anchor: anchor.map(Into::into),
            relation: "after".into(),
        }
        .encode()
    }
    fn range(&self, head: Option<&str>, tail: Option<&str>) -> TurnTimelineRange {
        TurnTimelineRange {
            turn_id: "turn".into(),
            is_first_session_turn: true,
            head_cursor: self.cursor(head),
            tail_cursor: tail.map(|id| self.cursor(Some(id))),
        }
    }
    fn history(
        &self,
    ) -> Arc<dyn pontia_application::client_contract::native_history::NativeHistory> {
        self.state
            .clients()
            .data("pi")
            .unwrap()
            .native_history(
                self.binding.clone(),
                Some(self.state.client_control().clone()),
            )
            .unwrap()
    }
    async fn peer(&self) -> (Arc<PiRpcPeer>, mpsc::Receiver<RpcRequest>, Arc<PiRpcPeer>) {
        let (left, right) = UnixStream::pair().unwrap();
        let (daemon, _) = PiRpcPeer::new(left);
        let (runtime, incoming) = PiRpcPeer::new(right);
        self.state
            .client_control()
            .attach("pi", "session", "runtime", "native", daemon.clone())
            .await
            .unwrap();
        (runtime, incoming, daemon)
    }
    async fn serve(
        &self,
        entries: Vec<Value>,
        page_size: usize,
    ) -> (Arc<AtomicUsize>, Arc<PiRpcPeer>) {
        let (peer, mut incoming, daemon) = self.peer().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        tokio::spawn(async move {
            while let Some(request) = incoming.recv().await {
                assert_eq!(request.method, "history.read");
                calls.fetch_add(1, Ordering::SeqCst);
                let start = request.params["continuation"]
                    .as_str()
                    .map_or(0, |s| s.parse::<usize>().unwrap());
                let end = (start + page_size).min(entries.len());
                peer.reply(request.id,json!({"session_id":"native","snapshot":"fixed","entry_count":entries.len(),"upper_entry_id":entries.last().map(|e| &e["id"]),"leaf_id":entries.last().map(|e| &e["id"]),"entries":entries[start..end],"continuation":if end<entries.len(){Some(end.to_string())}else{None}})).await.unwrap();
            }
        });
        (count, daemon)
    }
}
fn entry(id: &str, parent: Option<&str>, role: &str) -> Value {
    json!({"id":id,"parentId":parent,"type":"message","timestamp":"2026-10-11T00:00:00Z","message":{"role":role,"content":if role=="user"{json!(id)}else{json!([{"type":"text","text":id}])}}})
}

#[tokio::test]
async fn runtime_reads_and_branch_resolution_share_a_paginated_snapshot_without_a_session_file() {
    let f = Fixture::new().await;
    let (count, daemon) = f
        .serve(
            vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
            1,
        )
        .await;
    let history = f.history();
    let items = history
        .read_ranges(vec![f.range(None, Some("a"))])
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["u", "a"]
    );
    assert_eq!(
        history
            .branch_target(f.range(None, Some("a")))
            .await
            .unwrap(),
        "u"
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(!f.binding.client_session_file.as_ref().unwrap().exists());
    daemon.close();
}
#[tokio::test]
async fn selects_parent_chain_instead_of_append_order_and_does_not_persist_active_tail() {
    let f = Fixture::new().await;
    let (_, daemon) = f
        .serve(
            vec![
                entry("u", None, "user"),
                entry("a", Some("u"), "assistant"),
                entry("other", Some("a"), "user"),
                entry("other-a", Some("other"), "assistant"),
                entry("branch", Some("a"), "user"),
                entry("branch-a", Some("branch"), "assistant"),
            ],
            2,
        )
        .await;
    let mut range = f.range(Some("a"), None);
    range.is_first_session_turn = false;
    let items = f.history().read_ranges(vec![range]).await.unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["branch", "branch-a"]
    );
    daemon.close();
}
#[tokio::test]
async fn rejects_cross_scope_mixed_generation_and_nonancestor_ranges_before_reading_files() {
    let f = Fixture::new().await;
    let (_, daemon) = f
        .serve(
            vec![
                entry("u", None, "user"),
                entry("a", Some("u"), "assistant"),
                entry("branch", Some("u"), "assistant"),
            ],
            128,
        )
        .await;
    let mut wrong_binding = f.range(None, Some("a"));
    wrong_binding.head_cursor = PiEntryCursor {
        binding_id: "other".into(),
        session_id: "native".into(),
        anchor: None,
        relation: "after".into(),
    }
    .encode();
    let mut wrong_session = f.range(None, Some("a"));
    wrong_session.tail_cursor = Some(
        PiEntryCursor {
            binding_id: f.binding.id.clone(),
            session_id: "other".into(),
            anchor: Some("a".into()),
            relation: "after".into(),
        }
        .encode(),
    );
    let mut mixed = f.range(None, Some("a"));
    mixed.tail_cursor = Some(format!("pi-jsonl-v2:{}:0:after:a", f.binding.id));
    for range in [
        wrong_binding,
        wrong_session,
        mixed,
        f.range(Some("a"), Some("branch")),
        f.range(Some("missing"), Some("a")),
    ] {
        assert!(matches!(
            f.history().read_ranges(vec![range]).await,
            Err(TurnTimelineReadError::InvalidRange { .. })
        ));
    }
    daemon.close();
}
#[tokio::test]
async fn malformed_snapshots_fail_without_partial_results() {
    for kind in [
        "session",
        "duplicate",
        "parent",
        "leaf",
        "upper",
        "shape",
        "changing",
        "continuation",
        "omission",
        "count_changed",
        "count_limit",
    ] {
        let f = Fixture::new().await;
        let (peer, mut incoming, daemon) = f.peer().await;
        let task = tokio::spawn(async move {
            let mut n = 0;
            while let Some(request) = incoming.recv().await {
                let mut response = json!({"session_id":"native","snapshot":"fixed","entry_count":2,"upper_entry_id":"a","leaf_id":"a","entries":[entry("u",None,"user"),entry("a",Some("u"),"assistant")],"continuation":null});
                match kind {
                    "session" => response["session_id"] = json!("other"),
                    "duplicate" => response["entries"][1]["id"] = json!("u"),
                    "parent" => response["entries"][0]["parentId"] = json!("a"),
                    "leaf" => response["leaf_id"] = json!("missing"),
                    "upper" => response["upper_entry_id"] = json!("missing"),
                    // An omitted rooted/sibling entry leaves the returned parent chain valid.
                    "omission" => response["entry_count"] = json!(3),
                    "count_limit" => response["entry_count"] = json!(100_001),
                    "count_changed" => {
                        if n == 0 {
                            response["entries"] = json!([entry("u", None, "user")]);
                            response["continuation"] = json!("next");
                        } else {
                            response["entries"] = json!([entry("a", Some("u"), "assistant")]);
                            response["entry_count"] = json!(3);
                        }
                    }
                    "shape" => {
                        response.as_object_mut().unwrap().remove("leaf_id");
                    }
                    "changing" | "continuation" => {
                        response["entries"] = json!([entry("u", None, "user")]);
                        response["continuation"] = json!("next");
                        if n > 0 && kind == "changing" {
                            response["snapshot"] = json!("changed");
                        }
                    }
                    _ => {}
                };
                n += 1;
                peer.reply(request.id, response).await.unwrap();
            }
        });
        assert!(
            f.history()
                .read_ranges(vec![f.range(None, Some("a"))])
                .await
                .is_err(),
            "{kind}"
        );
        daemon.close();
        task.abort();
    }
}
#[tokio::test]
async fn unsupported_history_keeps_existing_control_available() {
    let f = Fixture::new().await;
    let (peer, mut incoming, daemon) = f.peer().await;
    tokio::spawn(async move {
        while let Some(request) = incoming.recv().await {
            if request.method == "history.read" {
                peer.reply_error(request.id, -32601, "unsupported")
                    .await
                    .unwrap();
            } else {
                peer.reply(request.id, json!({"pong":true})).await.unwrap();
            }
        }
    });
    assert!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
    f.state
        .client_control()
        .ping("session", "runtime")
        .await
        .unwrap();
    assert!(!daemon.is_closed());
    daemon.close();
}
#[tokio::test]
async fn missing_runtime_never_uses_an_existing_file_and_reconnect_restores_readability() {
    let mut f = Fixture::new().await;
    let path = f._root.path().join("available.jsonl");
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n",
            entry("u", None, "user"),
            entry("a", Some("u"), "assistant")
        ),
    )
    .unwrap();
    f.binding.client_session_file = Some(path);
    assert!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
    let (_, daemon) = f
        .serve(
            vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
            128,
        )
        .await;
    assert_eq!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .unwrap()
            .len(),
        2
    );
    daemon.close();
}
#[tokio::test]
async fn recovery_uses_native_anchors_without_creating_lifecycle_facts() {
    let f = Fixture::new().await;
    let (count, daemon) = f
        .serve(
            vec![
                entry("u", None, "user"),
                entry("a", Some("u"), "assistant"),
                entry("u2", Some("a"), "user"),
                entry("a2", Some("u2"), "assistant"),
            ],
            2,
        )
        .await;
    let history = f.history();
    let recovered = history
        .recover(vec![
            TurnHistoryCandidate {
                turn_id: "first".into(),
                input_summary: Some("u".into()),
                head_cursor: None,
                tail_cursor: None,
                state: TurnState::Abandoned,
                topology: TurnTopology::Unknown,
            },
            TurnHistoryCandidate {
                turn_id: "second".into(),
                input_summary: Some("u2".into()),
                head_cursor: Some(f.cursor(Some("a"))),
                tail_cursor: Some(f.cursor(Some("a2"))),
                state: TurnState::Completed,
                topology: TurnTopology::Unknown,
            },
        ])
        .await
        .unwrap();
    assert_eq!(recovered[0].head_cursor, Some(f.cursor(None)));
    assert_eq!(recovered[0].tail_cursor, Some(f.cursor(Some("a"))));
    assert!(
        matches!(recovered[1].topology.resolution,pontia_application::client_contract::TopologyResolution::Linked{ref parent_turn_id} if parent_turn_id=="first")
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
    daemon.close();
}

#[tokio::test]
async fn mixed_turns_keep_legacy_byte_ranges_and_only_new_turns_require_rpc() {
    use pontia_client_pi::raw_transcripts::{PiJsonlV2Cursor, TimelineBoundaryRelation};
    let mut f = Fixture::new().await;
    let old_entries = vec![
        entry("old-u", None, "user"),
        entry("old-a", Some("old-u"), "assistant"),
    ];
    let old_bytes = format!("{}\n{}\n", old_entries[0], old_entries[1]);
    let path = f._root.path().join("legacy.jsonl");
    std::fs::write(&path, &old_bytes).unwrap();
    f.binding.client_session_file = Some(path);
    let legacy_cursor = |offset, anchor: Option<&str>| {
        PiJsonlV2Cursor {
            binding_id: f.binding.id.clone(),
            byte_offset: offset,
            native_entry_anchor: anchor.map(Into::into),
            relation: TimelineBoundaryRelation::After,
        }
        .encode()
    };
    let old = TurnTimelineRange {
        turn_id: "first".into(),
        is_first_session_turn: true,
        head_cursor: legacy_cursor(0, None),
        tail_cursor: Some(legacy_cursor(old_bytes.len(), Some("old-a"))),
    };
    let mut new = f.range(Some("old-a"), Some("new-a"));
    new.turn_id = "second".into();
    new.is_first_session_turn = false;
    let mut all = old_entries;
    all.extend([
        entry("new-u", Some("old-a"), "user"),
        entry("new-a", Some("new-u"), "assistant"),
    ]);
    let (calls, daemon) = f.serve(all, 128).await;
    let history = f.history();
    assert_eq!(
        history.read_ranges(vec![old.clone()]).await.unwrap().len(),
        2
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let items = history
        .read_ranges(vec![old.clone(), new.clone()])
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["old-u", "old-a", "new-u", "new-a"]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    daemon.close();
    assert_eq!(f.history().read_ranges(vec![old]).await.unwrap().len(), 2);
    assert!(f.history().read_ranges(vec![new]).await.is_err());
}

#[tokio::test]
async fn connection_and_runtime_changes_invalidate_the_whole_snapshot_without_replay() {
    let f = Fixture::new().await;
    let (peer, mut incoming, daemon) = f.peer().await;
    let pending = tokio::spawn({
        let history = f.history();
        let range = f.range(None, Some("a"));
        async move { history.read_ranges(vec![range]).await }
    });
    let request = incoming.recv().await.unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id='session'")
        .execute(&f.state.db())
        .await
        .unwrap();
    peer.reply(request.id, json!({"session_id":"native","snapshot":"fixed","entry_count":2,"upper_entry_id":"a","leaf_id":"a","entries":[entry("u",None,"user"),entry("a",Some("u"),"assistant")],"continuation":null})).await.unwrap();
    assert!(pending.await.unwrap().is_err());
    assert!(incoming.try_recv().is_err());
    daemon.close();
}

#[tokio::test]
async fn reconnect_requires_a_new_read_handle_and_cannot_reuse_old_snapshot_identity() {
    let f = Fixture::new().await;
    let (_, old) = f
        .serve(
            vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
            128,
        )
        .await;
    let history = f.history();
    assert_eq!(
        history
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .unwrap()
            .len(),
        2
    );
    old.close();
    let (_, new) = f
        .serve(
            vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
            128,
        )
        .await;
    assert!(
        history
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
    assert_eq!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .unwrap()
            .len(),
        2
    );
    new.close();
}

#[tokio::test]
async fn assembles_history_larger_than_an_rpc_frame_from_bounded_pages() {
    let f = Fixture::new().await;
    let mut entries = Vec::new();
    for index in 0..8 {
        let id = format!("entry-{index}");
        let parent = (index > 0).then(|| format!("entry-{}", index - 1));
        let mut record = entry(&id, parent.as_deref(), "user");
        record["message"]["content"] = json!("中".repeat(120_000));
        entries.push(record);
    }
    let (calls, daemon) = f.serve(entries, 2).await;
    let items = f
        .history()
        .read_ranges(vec![f.range(None, Some("entry-7"))])
        .await
        .unwrap();
    assert_eq!(items.len(), 8);
    assert_eq!(items[7].item.content_preview.chars().count(), 120_000);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    daemon.close();
}

#[tokio::test]
async fn active_range_uses_snapshot_leaf_even_when_other_branches_were_appended_later() {
    let f = Fixture::new().await;
    let (peer, mut incoming, daemon) = f.peer().await;
    tokio::spawn(async move {
        let request = incoming.recv().await.unwrap();
        peer.reply(request.id,json!({"session_id":"native","snapshot":"fixed","entry_count":4,"upper_entry_id":"other-a","leaf_id":"a","entries":[entry("u",None,"user"),entry("a",Some("u"),"assistant"),entry("other",Some("a"),"user"),entry("other-a",Some("other"),"assistant")],"continuation":null})).await.unwrap();
        peer.closed().await;
    });
    let items = f
        .history()
        .read_ranges(vec![f.range(None, None)])
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["u", "a"]
    );
    daemon.close();
}

#[tokio::test]
async fn recovers_missing_head_and_abandoned_branch_switch_tail_from_long_unicode_inputs() {
    let f = Fixture::new().await;
    let base =
        json!({"id":"base","parentId":null,"type":"custom","timestamp":"2026-10-11T00:00:00Z"});
    let mut first = entry("u", Some("base"), "user");
    first["message"]["content"] = json!("界🙂".repeat(110));
    let mut next = entry("u2", Some("base"), "user");
    next["message"]["content"] = json!("新🦀".repeat(110));
    let (_, daemon) = f
        .serve(
            vec![
                base,
                first,
                entry("a", Some("u"), "assistant"),
                next,
                entry("a2", Some("u2"), "assistant"),
            ],
            2,
        )
        .await;
    let recovered = f
        .history()
        .recover(vec![
            TurnHistoryCandidate {
                turn_id: "first".into(),
                input_summary: Some("界🙂".repeat(100)),
                head_cursor: None,
                tail_cursor: None,
                state: TurnState::Abandoned,
                topology: TurnTopology::Unknown,
            },
            TurnHistoryCandidate {
                turn_id: "second".into(),
                input_summary: Some("新🦀".repeat(100)),
                head_cursor: Some(f.cursor(Some("base"))),
                tail_cursor: Some(f.cursor(Some("a2"))),
                state: TurnState::Completed,
                topology: TurnTopology::Root,
            },
        ])
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].turn_id, "first");
    assert_eq!(recovered[0].head_cursor, Some(f.cursor(Some("base"))));
    assert_eq!(recovered[0].tail_cursor, Some(f.cursor(Some("a"))));
    daemon.close();
}
