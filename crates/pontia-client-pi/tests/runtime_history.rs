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
        state.event_ingest_service().report_fact(pontia_application::ReportedFact { session_id: "session".into(), turn_id: None, report_key: None, fact_type: pontia_core::domain::EventType::SessionReady, data: json!({"runtime_id":"runtime", "client_session_key":"native", "client_cwd":root.path(), "client_session_file":root.path().join("does-not-exist.jsonl")}) }).await.unwrap();
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
    fn write_native(&mut self, native: &str, entries: Vec<Value>) {
        let path = self._root.path().join("persisted.jsonl");
        let mut bytes = format!("{}\n", json!({"type":"session", "version":3, "id":native}));
        for entry in entries {
            bytes.push_str(&format!("{entry}\n"));
        }
        std::fs::write(&path, bytes).unwrap();
        self.binding.client_session_file = Some(path);
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

fn checkpoint(id: &str, parent: Option<&str>) -> Value {
    json!({"id": id, "parentId": parent, "type":"compaction", "timestamp":"2026-10-11T00:00:00Z",
        "summary": "native private summary", "tokensBefore": 20000})
}

#[tokio::test]
async fn tool_calls_and_results_expose_the_same_native_call_id() {
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let f = Fixture::new().await;
    let (_, daemon) = f
        .serve(
            vec![
                entry("user", None, "user"),
                json!({
                    "id": "assistant",
                    "parentId": "user",
                    "type": "message",
                    "timestamp": "2026-10-11T00:00:01Z",
                    "message": {
                        "role": "assistant",
                        "content": [{
                            "type": "toolCall",
                            "id": "call-1",
                            "name": "read",
                            "arguments": {"path": "src/app.ts"}
                        }]
                    }
                }),
                json!({
                    "id": "result",
                    "parentId": "assistant",
                    "type": "message",
                    "timestamp": "2026-10-11T00:00:02Z",
                    "message": {
                        "role": "toolResult",
                        "toolCallId": "call-1",
                        "toolName": "read",
                        "content": [{"type": "text", "text": "contents"}],
                        "isError": false
                    }
                }),
            ],
            2,
        )
        .await;

    let items = f
        .history()
        .read_session_timeline(SessionTimelineSelection {
            turns: vec![SessionTimelineTurn {
                turn_id: "turn".into(),
                head_cursor: Some(f.cursor(None)),
                tail_cursor: Some(f.cursor(Some("result"))),
            }],
            selected_turn_ids: vec!["turn".into()],
        })
        .await
        .unwrap();
    let tool_items = items
        .iter()
        .filter(|item| matches!(item.item.kind.as_str(), "tool_call" | "tool_result"))
        .map(|item| (item.item.kind.as_str(), item.item.call_id.as_deref()))
        .collect::<Vec<_>>();

    assert_eq!(
        tool_items,
        vec![
            ("tool_call", Some("call-1")),
            ("tool_result", Some("call-1"))
        ]
    );
    daemon.close();
}

#[tokio::test]
async fn session_checkpoints_keep_native_order_and_turn_or_gap_page_ownership() {
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let f = Fixture::new().await;
    let (_, daemon) = f
        .serve(
            vec![
                entry("u1", None, "user"),
                checkpoint("inside", Some("u1")),
                entry("a1", Some("inside"), "assistant"),
                checkpoint("between", Some("a1")),
                entry("u2", Some("between"), "user"),
                entry("a2", Some("u2"), "assistant"),
                checkpoint("trailing", Some("a2")),
            ],
            2,
        )
        .await;
    let history = f.history();
    let selection = |selected: &[&str]| SessionTimelineSelection {
        turns: vec![
            SessionTimelineTurn {
                turn_id: "one".into(),
                head_cursor: Some(f.cursor(None)),
                tail_cursor: Some(f.cursor(Some("a1"))),
            },
            SessionTimelineTurn {
                turn_id: "two".into(),
                head_cursor: Some(f.cursor(Some("between"))),
                tail_cursor: Some(f.cursor(Some("a2"))),
            },
        ],
        selected_turn_ids: selected.iter().map(|id| (*id).into()).collect(),
    };
    let items = history
        .read_session_timeline(selection(&["one", "two"]))
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "user",
            "context_compaction",
            "assistant",
            "context_compaction",
            "user",
            "assistant",
            "context_compaction"
        ]
    );
    assert_eq!(
        items
            .iter()
            .map(|i| i.turn_id.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("one"),
            Some("one"),
            Some("one"),
            None,
            Some("two"),
            Some("two"),
            None
        ]
    );
    assert_eq!(items[1].item.content_preview, "native private summary");
    let first = history
        .read_session_timeline(selection(&["one"]))
        .await
        .unwrap();
    let second = history
        .read_session_timeline(selection(&["two"]))
        .await
        .unwrap();
    assert_eq!(first.len(), 3);
    assert_eq!(second.len(), 4);
    assert_eq!(
        first
            .into_iter()
            .chain(second)
            .map(|i| i.item.item_id)
            .collect::<Vec<_>>(),
        items
            .into_iter()
            .map(|i| i.item.item_id)
            .collect::<Vec<_>>()
    );
    daemon.close();
}

#[tokio::test]
async fn checkpoints_without_pontia_turns_come_only_from_the_native_branch() {
    use pontia_application::client_contract::raw_transcripts::SessionTimelineSelection;
    let f = Fixture::new().await;
    let (_, daemon) = f.serve(vec![
        entry("u", None, "user"),
        checkpoint("offbranch", Some("u")),
        checkpoint("first", Some("u")),
        json!({"id":"summary", "parentId":"first", "type":"message", "timestamp":"2026-10-11T00:00:00Z",
            "message":{"role":"compactionSummary", "summary":"second native summary"}}),
    ], 2).await;
    let items = f
        .history()
        .read_session_timeline(SessionTimelineSelection {
            turns: vec![],
            selected_turn_ids: vec![],
        })
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.item_id.as_str())
            .collect::<Vec<_>>(),
        vec!["pi:entry:first:block:0", "pi:entry:summary:block:0"]
    );
    assert!(
        items
            .iter()
            .all(|i| i.turn_id.is_none() && i.item.kind == "context_compaction")
    );
    daemon.close();
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
async fn missing_runtime_reads_persisted_entries_and_connected_runtime_includes_unflushed_entries()
{
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
    );
    let items = f
        .history()
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
    let (_, daemon) = f
        .serve(
            vec![
                entry("u", None, "user"),
                entry("a", Some("u"), "assistant"),
                entry("unflushed", Some("a"), "assistant"),
            ],
            128,
        )
        .await;
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
        vec!["u", "a", "unflushed"]
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
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let selection = |selected: &str| SessionTimelineSelection {
        turns: vec![old.clone(), new.clone()]
            .into_iter()
            .map(|range| SessionTimelineTurn {
                turn_id: range.turn_id,
                head_cursor: Some(range.head_cursor),
                tail_cursor: range.tail_cursor,
            })
            .collect(),
        selected_turn_ids: vec![selected.into()],
    };
    let legacy = f
        .history()
        .read_session_timeline(selection("first"))
        .await
        .unwrap();
    assert_eq!(
        legacy
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["old-u", "old-a"]
    );
    assert!(
        f.history()
            .read_session_timeline(selection("second"))
            .await
            .is_err()
    );
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

#[tokio::test]
async fn offline_ranges_follow_branches_across_blocks_and_resolve_branch_user() {
    let mut f = Fixture::new().await;
    let mut large = entry("large", Some("u"), "assistant");
    large["message"]["content"] = json!([{"type":"text", "text":"界🙂".repeat(20_000)}]);
    f.write_native(
        "native",
        vec![
            entry("base", None, "assistant"),
            entry("u", Some("base"), "user"),
            large,
            entry("offbranch", Some("base"), "user"),
            entry("a", Some("large"), "assistant"),
            entry("u2", Some("a"), "user"),
            entry("a2", Some("u2"), "assistant"),
        ],
    );
    let first = f.range(Some("base"), Some("a"));
    let mut second = f.range(Some("a"), Some("a2"));
    second.turn_id = "turn2".into();
    second.is_first_session_turn = false;
    let items = f
        .history()
        .read_ranges(vec![first.clone(), second])
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.item_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "pi:entry:u:block:0",
            "pi:entry:large:block:0",
            "pi:entry:a:block:0",
            "pi:entry:u2:block:0",
            "pi:entry:a2:block:0",
        ]
    );
    assert_eq!(items[1].item.content_preview, "界🙂".repeat(20_000));
    assert_eq!(f.history().branch_target(first).await.unwrap(), "u");
}

#[tokio::test]
async fn offline_recent_range_does_not_parse_unrelated_older_records() {
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![
            entry("base", None, "assistant"),
            entry("u", Some("base"), "user"),
            entry("a", Some("u"), "assistant"),
        ],
    );
    let path = f.binding.client_session_file.as_ref().unwrap();
    let bytes = std::fs::read_to_string(path).unwrap();
    let (header, rest) = bytes.split_once('\n').unwrap();
    std::fs::write(
        path,
        format!("{header}\nnot-json-in-an-older-record\n{rest}"),
    )
    .unwrap();
    let items = f
        .history()
        .read_ranges(vec![f.range(Some("base"), Some("a"))])
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["u", "a"]
    );
}

#[tokio::test]
async fn offline_history_rejects_wrong_identity_missing_anchors_and_invalid_parent_chains() {
    let mut f = Fixture::new().await;
    f.write_native(
        "other",
        vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
    );
    assert!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
    f.write_native(
        "native",
        vec![
            entry("u", None, "user"),
            entry("a", Some("u"), "assistant"),
            entry("sibling", Some("u"), "assistant"),
        ],
    );
    for range in [
        f.range(None, Some("missing")),
        f.range(Some("missing"), Some("a")),
        f.range(Some("a"), Some("sibling")),
    ] {
        assert!(matches!(
            f.history().read_ranges(vec![range]).await,
            Err(TurnTimelineReadError::InvalidRange { .. })
        ));
    }
    f.write_native(
        "native",
        vec![
            entry("u", None, "user"),
            entry("x", Some("u"), "assistant"),
            entry("x", Some("x"), "assistant"),
            entry("a", Some("x"), "assistant"),
        ],
    );
    assert!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn online_failure_does_not_fall_back_to_a_valid_file() {
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
    );
    let (peer, mut incoming, daemon) = f.peer().await;
    tokio::spawn(async move {
        let request = incoming.recv().await.unwrap();
        peer.reply(request.id, json!({"session_id":"wrong"}))
            .await
            .unwrap();
        peer.closed().await;
    });
    assert!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
    daemon.close();
}

#[tokio::test]
async fn offline_handle_switches_to_runtime_when_a_connection_becomes_available() {
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
    );
    let history = f.history();
    assert_eq!(
        history
            .read_ranges(vec![f.range(None, None)])
            .await
            .unwrap()
            .len(),
        2
    );
    let (_, daemon) = f
        .serve(
            vec![
                entry("u", None, "user"),
                entry("a", Some("u"), "assistant"),
                entry("unflushed", Some("a"), "assistant"),
            ],
            128,
        )
        .await;
    let items = history
        .read_ranges(vec![f.range(None, None)])
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["u", "a", "unflushed"]
    );
    daemon.close();
}

#[tokio::test]
async fn offline_session_pages_keep_multiple_gap_checkpoints_and_ignore_unselected_open_turns() {
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![
            entry("u1", None, "user"),
            checkpoint("inside", Some("u1")),
            entry("a1", Some("inside"), "assistant"),
            checkpoint("gap1", Some("a1")),
            checkpoint("gap2", Some("gap1")),
            entry("u2", Some("gap2"), "user"),
            entry("a2", Some("u2"), "assistant"),
            checkpoint("trailing", Some("a2")),
        ],
    );
    let selection = |ids: &[&str], first_open: bool| SessionTimelineSelection {
        turns: vec![
            SessionTimelineTurn {
                turn_id: "one".into(),
                head_cursor: Some(f.cursor(None)),
                tail_cursor: (!first_open).then(|| f.cursor(Some("a1"))),
            },
            SessionTimelineTurn {
                turn_id: "two".into(),
                head_cursor: Some(f.cursor(Some("gap2"))),
                tail_cursor: Some(f.cursor(Some("a2"))),
            },
        ],
        selected_turn_ids: ids.iter().map(|id| (*id).into()).collect(),
    };
    let history = f.history();
    let second = history
        .read_session_timeline(selection(&["two"], false))
        .await
        .unwrap();
    assert_eq!(
        second
            .iter()
            .map(|i| i.item.item_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "pi:entry:gap1:block:0",
            "pi:entry:gap2:block:0",
            "pi:entry:u2:block:0",
            "pi:entry:a2:block:0",
            "pi:entry:trailing:block:0"
        ]
    );
    assert_eq!(
        second
            .iter()
            .map(|i| i.turn_id.as_deref())
            .collect::<Vec<_>>(),
        vec![None, None, Some("two"), Some("two"), None]
    );
    let first = history
        .read_session_timeline(selection(&["one"], false))
        .await
        .unwrap();
    assert_eq!(
        first
            .iter()
            .map(|i| i.item.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["user", "context_compaction", "assistant"]
    );
    let all = history
        .read_session_timeline(selection(&["one", "two"], false))
        .await
        .unwrap();
    assert_eq!(
        first
            .into_iter()
            .chain(second)
            .map(|i| i.item.item_id)
            .collect::<Vec<_>>(),
        all.into_iter().map(|i| i.item.item_id).collect::<Vec<_>>()
    );
    let second = history
        .read_session_timeline(selection(&["two"], true))
        .await
        .unwrap();
    assert!(
        second
            .iter()
            .any(|i| i.item.item_id == "pi:entry:a2:block:0")
    );
}

#[tokio::test]
async fn offline_session_without_turns_only_returns_checkpoints_on_the_persisted_branch() {
    use pontia_application::client_contract::raw_transcripts::SessionTimelineSelection;
    let mut f = Fixture::new().await;
    f.write_native("native", vec![entry("u", None, "user"), checkpoint("offbranch", Some("u")), checkpoint("first", Some("u")),
        json!({"id":"summary", "parentId":"first", "type":"message", "timestamp":"2026-10-11T00:00:00Z", "message":{"role":"compactionSummary", "summary":"native summary"}}),
    ]);
    let items = f
        .history()
        .read_session_timeline(SessionTimelineSelection {
            turns: vec![],
            selected_turn_ids: vec![],
        })
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.item_id.as_str())
            .collect::<Vec<_>>(),
        vec!["pi:entry:first:block:0", "pi:entry:summary:block:0"]
    );
    assert!(
        items
            .iter()
            .all(|i| i.turn_id.is_none() && i.item.kind == "context_compaction")
    );
}

#[tokio::test]
async fn offline_page_ignores_invalid_unselected_chains_before_assigning_ownership() {
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![
            entry("root", None, "user"),
            entry("left", Some("root"), "assistant"),
            entry("right", Some("root"), "user"),
            entry("leaf", Some("right"), "assistant"),
        ],
    );
    let items = f
        .history()
        .read_session_timeline(SessionTimelineSelection {
            turns: vec![
                SessionTimelineTurn {
                    turn_id: "invalid".into(),
                    head_cursor: Some(f.cursor(Some("left"))),
                    tail_cursor: Some(f.cursor(Some("leaf"))),
                },
                SessionTimelineTurn {
                    turn_id: "selected".into(),
                    head_cursor: Some(f.cursor(Some("right"))),
                    tail_cursor: Some(f.cursor(Some("leaf"))),
                },
            ],
            selected_turn_ids: vec!["selected".into()],
        })
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["leaf"]
    );
}

#[tokio::test]
async fn offline_page_keeps_unselected_head_boundaries_when_their_tails_are_invalid() {
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![
            entry("u1", None, "user"),
            entry("a1", Some("u1"), "assistant"),
            checkpoint("gap1", Some("a1")),
            checkpoint("gap2", Some("gap1")),
            entry("u2", Some("gap2"), "user"),
            entry("a2", Some("u2"), "assistant"),
            checkpoint("trailing", Some("a2")),
        ],
    );
    let items = f
        .history()
        .read_session_timeline(SessionTimelineSelection {
            turns: vec![
                SessionTimelineTurn {
                    turn_id: "one".into(),
                    head_cursor: Some(f.cursor(None)),
                    tail_cursor: Some(f.cursor(Some("a1"))),
                },
                SessionTimelineTurn {
                    turn_id: "two".into(),
                    head_cursor: Some(f.cursor(Some("gap2"))),
                    tail_cursor: Some(f.cursor(None)),
                },
            ],
            selected_turn_ids: vec!["one".into()],
        })
        .await
        .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| i.item.content_preview.as_str())
            .collect::<Vec<_>>(),
        vec!["u1", "a1"]
    );
}

#[tokio::test]
async fn offline_history_does_not_return_partial_results_from_an_incomplete_file() {
    use std::io::Write;
    let mut f = Fixture::new().await;
    f.write_native(
        "native",
        vec![entry("u", None, "user"), entry("a", Some("u"), "assistant")],
    );
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(f.binding.client_session_file.as_ref().unwrap())
        .unwrap();
    file.write_all(b"{\"id\":\"partial").unwrap();
    assert!(
        f.history()
            .read_ranges(vec![f.range(None, Some("a"))])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn offline_empty_first_turn_is_empty_but_explicit_missing_anchors_are_rejected() {
    use pontia_application::client_contract::raw_transcripts::{
        SessionTimelineSelection, SessionTimelineTurn,
    };
    let mut f = Fixture::new().await;
    f.write_native("native", vec![]);
    let history = f.history();
    assert!(
        history
            .read_ranges(vec![f.range(None, None)])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        history
            .read_session_timeline(SessionTimelineSelection {
                turns: vec![SessionTimelineTurn {
                    turn_id: "turn".into(),
                    head_cursor: Some(f.cursor(None)),
                    tail_cursor: None
                }],
                selected_turn_ids: vec!["turn".into()],
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        history
            .read_ranges(vec![f.range(None, Some("missing"))])
            .await,
        Err(TurnTimelineReadError::InvalidRange { .. })
    ));
}
