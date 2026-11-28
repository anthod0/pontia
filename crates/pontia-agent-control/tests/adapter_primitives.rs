use pontia_agent_control::{
    AgentControl, ControlQueryService, SessionMetadataStore,
    runtime::{AttachmentBinding, AttachmentProcess, RuntimeAttachmentService},
    turns::NativeTurnService,
};
use pontia_core::{
    Error,
    domain::{EventType, SessionState, TurnState},
};
use pontia_runtime::ProcessFingerprint;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn database() -> (TempDir, SqlitePool) {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("control.db").display());
    let pool = pontia_storage_sqlite::connect_sqlite(&url).await.unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('s','client','starting')",
    )
    .execute(&pool)
    .await
    .unwrap();
    (root, pool)
}

#[tokio::test]
async fn session_reads_filter_state_client_and_literal_metadata_key() {
    let (_root, pool) = database().await;
    sqlx::raw_sql(r#"UPDATE sessions SET metadata='{"model":"m","reasoning_effort":"high","root.dir":"a"}',runtime_environment='{"X":"v"}',credential_env='TOKEN',workspace_ref='/work';
        INSERT INTO sessions(session_id,client_type,state,metadata) VALUES ('done','client','exited','{"root.dir":"a"}'),('different','other','starting','{"root.dir":"a"}');"#)
    .execute(&pool).await.unwrap();
    let queries = ControlQueryService::new(pool);
    assert_eq!(
        queries.session_state("s").await.unwrap(),
        Some(SessionState::Starting)
    );
    assert_eq!(queries.session_state("missing").await.unwrap(), None);
    let settings = queries.session_launch_settings("s").await.unwrap().unwrap();
    assert_eq!(settings.model.as_deref(), Some("m"));
    assert_eq!(settings.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(settings.runtime_environment["X"], "v");
    assert_eq!(settings.credential_env.as_deref(), Some("TOKEN"));
    assert_eq!(
        queries
            .list_sessions_by_client("client", &[SessionState::Starting], Some(("root.dir", "a")))
            .await
            .unwrap(),
        ["s"]
    );
    assert_eq!(
        queries
            .list_sessions_by_client("client", &[], None)
            .await
            .unwrap(),
        ["done", "s"]
    );
    let shutdown = queries
        .list_active_shutdown_sessions("client")
        .await
        .unwrap();
    assert_eq!(shutdown.len(), 1);
    assert_eq!(shutdown[0].workspace.as_deref(), Some("/work"));
}

#[tokio::test]
async fn metadata_updates_preserve_literal_keys_json_types_and_other_writers() {
    let (_root, pool) = database().await;
    let store = SessionMetadataStore::new(pool.clone());
    store
        .patch_keys(
            "s",
            &[
                ("a.b", json!([true])),
                ("count", Value::Null),
                ("a", json!({"b":1})),
            ],
        )
        .await
        .unwrap();
    store
        .update_key("s", "count", |old| {
            assert_eq!(old, Some(Value::Null));
            Ok(json!(0))
        })
        .await
        .unwrap();
    let mut writers = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let store = store.clone();
        writers.spawn(async move {
            store
                .update_key("s", "count", |old| {
                    Ok(json!(old.unwrap().as_u64().unwrap() + 1))
                })
                .await
        });
    }
    store.remove_keys("s", &["a.b"]).await.unwrap();
    while let Some(writer) = writers.join_next().await {
        writer.unwrap().unwrap();
    }
    let result = store
        .update_key("s", "count", |_| Err(Error::Domain("cancel".into())))
        .await;
    assert!(result.is_err());
    assert_eq!(
        ControlQueryService::new(pool)
            .session_metadata("s")
            .await
            .unwrap(),
        Some(json!({"a":{"b":1},"count":12}))
    );
}

fn peer(pid: u32) -> ProcessFingerprint {
    ProcessFingerprint {
        pid,
        boot_time_seconds: 1,
        start_time_seconds: 2,
    }
}

async fn starting_runtime(pool: &SqlitePool) {
    sqlx::raw_sql(r#"INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at) VALUES('r','s','host','starting','now');
    UPDATE sessions SET metadata='{"incompatible_control_runtime":"r","keep":1}' WHERE session_id='s';"#)
    .execute(pool).await.unwrap();
}

#[tokio::test]
async fn failed_marker_cleanup_rolls_back_fingerprint_backfill() {
    let (_root, pool) = database().await;
    starting_runtime(&pool).await;
    sqlx::query(
        "CREATE TRIGGER fail_cleanup BEFORE UPDATE OF metadata ON sessions BEGIN SELECT RAISE(ABORT,'reject cleanup'); END",
    )
    .execute(&pool).await.unwrap();
    let service = RuntimeAttachmentService::new(pool.clone());
    let process = peer(7);
    let attach = || {
        service.confirm_attachment(
            "s",
            "r",
            "client",
            "host",
            AttachmentBinding::Launch,
            AttachmentProcess::BackfillIfStarting(&process),
        )
    };
    assert!(matches!(attach().await, Err(Error::Database(_))));
    let stored: Option<String> =
        sqlx::query_scalar("SELECT process_fingerprint FROM session_runtimes WHERE runtime_id='r'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, None);
    sqlx::query("DROP TRIGGER fail_cleanup")
        .execute(&pool)
        .await
        .unwrap();
    attach().await.unwrap();
    assert_eq!(
        service
            .pending_ready_fingerprint("s", "r", "client", "native")
            .await
            .unwrap(),
        Some(peer(7))
    );
    assert_eq!(
        ControlQueryService::new(pool.clone())
            .session_metadata("s")
            .await
            .unwrap(),
        Some(json!({"keep":1}))
    );
    let pending = |process| {
        service.confirm_attachment(
            "s",
            "r",
            "client",
            "host",
            AttachmentBinding::PendingReady("native"),
            AttachmentProcess::Confirmed(process),
        )
    };
    pending(&process).await.unwrap();
    let wrong = peer(8);
    assert!(matches!(
        pending(&wrong).await,
        Err(Error::StateConflict(_))
    ));
    sqlx::query(
        "INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key) VALUES ('b','s','client','/','other')",
    )
    .execute(&pool).await.unwrap();
    assert!(matches!(
        pending(&process).await,
        Err(Error::StateConflict(_))
    ));
    service
        .confirm_attachment(
            "s",
            "r",
            "client",
            "host",
            AttachmentBinding::Native("other"),
            AttachmentProcess::Confirmed(&process),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn confirmation_observes_concurrent_exit_before_writing() {
    let (_root, pool) = database().await;
    starting_runtime(&pool).await;
    let service = RuntimeAttachmentService::new(pool.clone());
    sqlx::query("UPDATE sessions SET state='error'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        service
            .active_control_runtime_id("s", "host")
            .await
            .unwrap()
            .as_deref(),
        Some("r")
    );
    let mut exit = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::raw_sql(
        "UPDATE session_runtimes SET state='exited'; UPDATE sessions SET state='exited';",
    )
    .execute(&mut *exit)
    .await
    .unwrap();
    let process = peer(7);
    let attach = service.confirm_attachment(
        "s",
        "r",
        "client",
        "host",
        AttachmentBinding::Launch,
        AttachmentProcess::BackfillIfStarting(&process),
    );
    let (attached, committed) = tokio::join!(biased; attach,exit.commit());
    committed.unwrap();
    assert!(matches!(attached, Err(Error::StateConflict(_))));
    assert!(service.session_exited_with_runtime("s", "r").await.unwrap());
    assert!(
        !service
            .session_exited_with_runtime("s", "other")
            .await
            .unwrap()
    );
    assert_eq!(
        service
            .active_control_runtime_id("s", "host")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        ControlQueryService::new(pool)
            .session_metadata("s")
            .await
            .unwrap()
            .unwrap()["incompatible_control_runtime"],
        "r"
    );
}

#[tokio::test]
async fn native_turn_mapping_is_session_scoped_and_tracks_running_state() {
    let (root, pool) = database().await;
    sqlx::raw_sql(
        "INSERT INTO turns(session_id,turn_id,state) VALUES ('s','t','running');
        INSERT INTO native_turn_bindings(session_id,turn_id,client_turn_id) VALUES ('s','t','n');",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = AgentControl::builder(pool.clone(), root.path().into()).build();
    let native = NativeTurnService::new(pool.clone(), app.event_ingest_service());
    assert_eq!(
        native.client_turn_id("s", "t").await.unwrap().as_deref(),
        Some("n")
    );
    assert_eq!(
        native.turn_id("s", "n").await.unwrap().as_deref(),
        Some("t")
    );
    assert!(!native.is_bound("other", "n").await.unwrap());
    assert_eq!(
        native.running_turn_id("s", "n").await.unwrap().as_deref(),
        Some("t")
    );
    sqlx::query("UPDATE turns SET state='completed' WHERE turn_id='t'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(native.running_turn_id("s", "n").await.unwrap(), None);
    assert!(native.is_bound("s", "n").await.unwrap());
}

#[tokio::test]
async fn event_queries_distinguish_session_replay_from_missing_reports_and_return_turn_boundaries()
{
    let (_root, pool) = database().await;
    sqlx::raw_sql(r#"INSERT INTO turns(session_id,turn_id,state) VALUES ('s','first','failed'),('s','active','running');
        INSERT INTO events(event_id,session_id,turn_id,source,client_type,event_type,occurred_at,payload,report_key,timeline_boundary) VALUES
        ('1','s',NULL,'agent_client','client','session.ready','now','{}','repeat',NULL),
        ('2','s','first','agent_client','client','turn.started','now','{"input":1}','repeat','{"cursor":"head"}'),
        ('3','s','first','agent_client','client','turn.failed','now','{"failure":{"message":"failed"}}',NULL,'{"cursor":"tail"}'),
        ('late','s','first','agent_client','client','turn.failed','now','{"failure":{"message":"final reason"}}',NULL,NULL),
        ('4','s','active','agent_client','client','turn.started','now','{"input":2}','second',NULL);"#)
    .execute(&pool).await.unwrap();
    let queries = ControlQueryService::new(pool);
    assert_eq!(
        queries
            .find_by_report_key("s", EventType::SessionReady, "repeat")
            .await
            .unwrap()
            .unwrap()
            .turn_id,
        None
    );
    assert!(
        queries
            .find_by_report_key("other", EventType::SessionReady, "repeat")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        queries
            .find_by_report_key("s", EventType::SessionExited, "repeat")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        queries
            .find_turn_by_report_key("s", EventType::TurnStarted, "repeat")
            .await
            .unwrap(),
        Some(("first".into(), TurnState::Failed))
    );
    assert_eq!(
        queries
            .list_events_by_type("s", EventType::TurnStarted, Some("first"))
            .await
            .unwrap(),
        [json!({"input":1})]
    );
    assert_eq!(
        queries
            .list_turn_payloads_by_type("s", EventType::TurnStarted)
            .await
            .unwrap(),
        [
            ("first".into(), json!({"input":1})),
            ("active".into(), json!({"input":2}))
        ]
    );
    assert_eq!(
        queries
            .latest_event("s", EventType::TurnStarted)
            .await
            .unwrap(),
        Some(json!({"input":2}))
    );
    assert_eq!(
        queries.active_turn_started_event("s").await.unwrap(),
        Some(("active".into(), json!({"input":2})))
    );
    let turns = queries
        .completed_turns_before_with_boundaries("s", "active")
        .await
        .unwrap();
    assert_eq!(turns.len(), 1);
    let turn = &turns[0];
    assert_eq!(
        (&*turn.turn_id, &*turn.start_cursor, &*turn.end_cursor),
        ("first", "head", "tail")
    );
    assert_eq!(turn.failure_message.as_deref(), Some("final reason"));
}

#[tokio::test]
async fn failure_search_resets_on_ready_and_skips_events_without_a_message() {
    let (_root, pool) = database().await;
    let queries = ControlQueryService::new(pool.clone());
    let failures = [EventType::SessionError, EventType::RuntimeExited];
    let latest = || queries.latest_failure_after_event("s", EventType::RuntimeReady, &failures);
    for (id, kind, payload, expected) in [
        (
            "1",
            "session.error",
            json!({"failure":{"message":"old"}}),
            Some("old"),
        ),
        ("2", "runtime.ready", json!({}), None),
        (
            "3",
            "runtime.exited",
            json!({"failure":{"message":"new"}}),
            Some("new"),
        ),
        ("4", "session.error", json!({}), Some("new")),
    ] {
        sqlx::query(
            "INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at,payload) VALUES (?,'s','agent_client','client',?,'now',?)",
        )
        .bind(id)
        .bind(kind)
        .bind(payload)
        .execute(&pool).await.unwrap();
        assert_eq!(latest().await.unwrap().as_deref(), expected);
    }
}

#[tokio::test]
async fn shared_protocol_gate_accepts_current_version_and_rejects_an_old_peer() {
    use pontia_agent_control::ipc::{
        protocol::rejection,
        rpc::{PROTOCOL_VERSION, RpcRequest},
    };
    let (root, pool) = database().await;
    sqlx::raw_sql(
        "UPDATE sessions SET client_type='generic' WHERE session_id='s';
        INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at)
        VALUES ('r','s','generic_control','starting','now');",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut clients = pontia_agent_control::clients::ClientRegistry::default();
    clients.register(pontia_agent_control::client_contract::test_registration());
    let app = AgentControl::builder(pool, root.path().into())
        .clients(clients)
        .build();
    let mut request = RpcRequest {
        id: None,
        method: "runtime.attach".into(),
        params: json!({"version":PROTOCOL_VERSION,"session_id":"s","runtime_id":"r"}),
    };
    let pid = Some(std::process::id());
    assert!(rejection(&app, pid, &request).await.is_none());
    request.params["version"] = json!(1);
    assert!(rejection(&app, pid, &request).await.is_some());
    assert_eq!(
        app.queries().session_metadata("s").await.unwrap().unwrap()["incompatible_control_runtime"],
        "r"
    );
}
