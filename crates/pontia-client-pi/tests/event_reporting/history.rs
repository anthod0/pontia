use super::*;
use pontia_application::TurnTimelineService;
use pontia_core::domain::{ProjectionState, TurnState};
use std::io::Write;

impl Fixture {
    async fn history_source(&self) -> std::path::PathBuf {
        sqlx::query("UPDATE runtime_bindings SET capabilities=? WHERE session_id='session'")
            .bind(serde_json::to_string(&pontia_client_pi::CAPABILITIES).unwrap())
            .execute(&self.state.db())
            .await
            .unwrap();
        let path = self.root.path().join("native.jsonl");
        std::fs::write(&path, b"").unwrap();
        AgentBindingService::new(self.state.db())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: "session".into(),
                client_type: "pi".into(),
                launch_cwd: self.root.path().display().to_string(),
                client_session_key: "native".into(),
                client_session_file: Some(path.display().to_string()),
                metadata: json!({}),
            })
            .await
            .unwrap();
        path
    }

    async fn history_start(&self, previous: Option<&str>) -> String {
        self.report(
            EventType::TurnStarted,
            None,
            json!({
                "runtime_instance_id":"runtime", "timeline_anchor":{"previous_leaf_id":previous}
            }),
        )
        .await
        .unwrap()
        .turn_id
        .unwrap()
    }

    async fn history_complete(&self, turn: &str, leaf: &str) {
        self.report(
            EventType::TurnCompleted,
            Some(turn),
            json!({
                "runtime_instance_id":"runtime", "timeline_anchor":{"terminal_leaf_id":leaf}
            }),
        )
        .await
        .unwrap();
    }

    async fn history_crash(&self) {
        self.state
            .event_ingest_service()
            .ingest_runtime_observation_event(PontiaEvent::new(
                "session",
                None,
                PontiaEventSource::RuntimeManager,
                "pi",
                PontiaEventType::SessionExited,
                json!({"runtime_instance_id":"runtime"}),
            ))
            .await
            .unwrap();
    }

    async fn history_resume(&self) {
        self.state
            .event_ingest_service()
            .ingest_pontia_event(PontiaEvent::new(
                "session",
                None,
                PontiaEventSource::ExternalApi,
                "pi",
                PontiaEventType::SessionResuming,
                json!({}),
            ))
            .await
            .unwrap();
        self.report(
            EventType::SessionReady,
            None,
            json!({"runtime_instance_id":"runtime", "client_session_key":"native", "client_cwd":self.root.path(), "client_session_file":self.root.path().join("native.jsonl")}),
        )
        .await
        .unwrap();
    }
}

fn append(path: &std::path::Path, id: &str, parent: Option<&str>, role: &str) {
    let record = json!({"id":id, "parentId":parent, "type":"message", "message":{
        "role":role, "content":[{"type":"text","text":id}]
    }});
    writeln!(
        std::fs::OpenOptions::new().append(true).open(path).unwrap(),
        "{record}"
    )
    .unwrap();
}

#[tokio::test]
async fn crash_recovery_preserves_facts_and_supports_updates_refresh_pagination_and_replay() {
    let fixture = Fixture::new("pi").await;
    let path = fixture.history_source().await;
    let service = fixture.state.event_ingest_service();
    let binding_before = AgentBindingService::new(fixture.state.db())
        .binding_for_session("session")
        .await
        .unwrap()
        .unwrap();
    let mut published = fixture.state.agent_events().subscribe();
    let first = fixture.history_start(None).await;
    assert_eq!(
        published.try_recv().unwrap().event_type,
        EventType::TurnStarted
    );
    assert_eq!(
        published.try_recv().unwrap().event_type,
        EventType::TurnTopologyRecovered
    );
    append(&path, "user1", None, "user");
    append(&path, "answer1", Some("user1"), "assistant");
    fixture.history_complete(&first, "answer1").await;
    let crashed = fixture.history_start(Some("answer1")).await;
    append(&path, "crash-user", Some("answer1"), "user");
    append(&path, "tool-call", Some("crash-user"), "assistant");
    fixture.history_crash().await;
    let abandoned = service.get_turn(&crashed).await.unwrap().unwrap();
    assert_eq!(abandoned.state, TurnState::Abandoned);
    assert!(abandoned.tail_cursor.is_none());
    let partial = TurnTimelineService::new(service.clone())
        .tree_history("session".into(), None, 20)
        .await
        .unwrap();
    assert_eq!(
        partial.groups[0].items.last().unwrap().item.content_preview,
        "answer1"
    );
    assert_eq!(
        serde_json::to_value(partial.groups.last().unwrap()).unwrap()["history_issue"],
        "range_unavailable"
    );
    fixture.history_resume().await;
    let mut turns = vec![first.clone(), crashed.clone()];
    let mut previous = "tool-call".to_string();
    for index in 0..21 {
        let turn = fixture.history_start(Some(&previous)).await;
        let user = format!("user-{index}");
        let answer = format!("answer-{index}");
        append(&path, &user, Some(&previous), "user");
        append(&path, &answer, Some(&user), "assistant");
        fixture.history_complete(&turn, &answer).await;
        let updates = TurnTimelineService::new(service.clone())
            .tree_updates("session".into(), turns.last().cloned())
            .await
            .unwrap();
        assert_eq!(
            updates
                .groups
                .last()
                .unwrap()
                .items
                .last()
                .unwrap()
                .item
                .content_preview,
            answer
        );
        assert!(
            updates.groups.iter().all(|g| g.history_issue.is_none()),
            "{updates:?}"
        );
        turns.push(turn);
        previous = answer;
    }
    let recovered = service.get_turn(&crashed).await.unwrap().unwrap();
    assert!(recovered.tail_cursor.is_some());
    assert_eq!(recovered.state, abandoned.state);
    assert_eq!(recovered.state_version, abandoned.state_version);
    assert_eq!(recovered.metadata, abandoned.metadata);
    assert_eq!(recovered.output_summary, abandoned.output_summary);
    let binding_after = AgentBindingService::new(fixture.state.db())
        .binding_for_session("session")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding_before.id, binding_after.id);
    assert_eq!(
        binding_before.client_session_key,
        binding_after.client_session_key
    );
    // Recreate the persisted pre-fix shape: the crash has no tail and all later
    // starts carry Unknown. Keep the original lifecycle events for replay.
    sqlx::query("DELETE FROM events WHERE event_type='turn.timeline_boundary_recovered' OR (event_type='turn.topology_recovered' AND turn_id > ?)")
        .bind(&crashed).execute(&fixture.state.db()).await.unwrap();
    let mut legacy = ProjectionState::default();
    for event in service.list_events("session").await.unwrap() {
        legacy.apply(&event).unwrap();
    }
    let mut legacy_turns = legacy.turns().collect::<Vec<_>>();
    legacy_turns.sort_by(|a, b| a.turn_id.cmp(&b.turn_id));
    let mut tx = fixture.state.db().begin().await.unwrap();
    sqlx::query("DELETE FROM turns WHERE session_id='session'")
        .execute(&mut *tx)
        .await
        .unwrap();
    for t in legacy_turns {
        pontia_storage_sqlite::repositories::turns::SqliteTurnRepository::upsert_projection_in_tx(
            &mut tx,
            pontia_storage_sqlite::repositories::turns::TurnProjectionUpsertRecord {
                turn_id: t.turn_id.clone(),
                session_id: t.session_id.clone(),
                head_cursor: t.head_cursor.clone(),
                tail_cursor: t.tail_cursor.clone(),
                parent_turn_id: t.topology.parent_turn_id().map(str::to_owned),
                topology_status: t.topology.status().into(),
                state: t.state.to_string(),
                state_version: t.state_version,
                input_summary: t.input_summary.clone(),
                output_summary: t.output_summary.clone(),
                metadata: serde_json::to_string(&t.metadata).unwrap(),
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let history = TurnTimelineService::new(service.clone());
    let newest = history
        .tree_history("session".into(), None, 20)
        .await
        .unwrap();
    assert_eq!(newest.groups.len(), 20);
    let older = history
        .tree_history("session".into(), newest.next_from_turn_id, 20)
        .await
        .unwrap();
    assert_eq!(
        older.groups.iter().map(|g| &g.turn_id).collect::<Vec<_>>(),
        turns[..3].iter().collect::<Vec<_>>()
    );
    assert!(older.next_from_turn_id.is_none());
    assert!(older.groups.iter().all(|g| g.history_issue.is_none()));
    let events = service.list_events("session").await.unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == EventType::TurnTopologyRecovered)
    );
    assert!(!events.iter().any(
        |e| e.turn_id.as_deref() == Some(&crashed) && e.event_type == EventType::TurnCompleted
    ));
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    for turn in turns {
        assert_eq!(
            replay.turn(&turn),
            service.get_turn(&turn).await.unwrap().as_ref()
        );
    }
    // Reopening history uses committed repairs and does not append duplicate events.
    history
        .tree_history("session".into(), None, 100)
        .await
        .unwrap();
    assert_eq!(
        service.list_events("session").await.unwrap().len(),
        events.len()
    );
}

#[tokio::test]
async fn unresolvable_ancestry_does_not_hide_verified_later_turns() {
    let fixture = Fixture::new("pi").await;
    let path = fixture.history_source().await;
    append(&path, "untracked-user", None, "user");
    append(
        &path,
        "untracked-answer",
        Some("untracked-user"),
        "assistant",
    );
    let turn = fixture.history_start(Some("untracked-answer")).await;
    append(&path, "tracked-user", Some("untracked-answer"), "user");
    append(&path, "tracked-answer", Some("tracked-user"), "assistant");
    fixture.history_complete(&turn, "tracked-answer").await;
    let history = TurnTimelineService::new(fixture.state.event_ingest_service());
    let page = history
        .tree_history("session".into(), None, 20)
        .await
        .unwrap();
    assert_eq!(page.groups.len(), 1);
    assert_eq!(
        page.groups[0].items.last().unwrap().item.content_preview,
        "tracked-answer"
    );
    assert_eq!(
        serde_json::to_value(&page.groups[0]).unwrap()["history_issue"],
        "topology_unknown"
    );
    assert_eq!(
        fixture
            .state
            .event_ingest_service()
            .get_turn(&turn)
            .await
            .unwrap()
            .unwrap()
            .topology,
        TurnTopology::Unknown
    );
}
