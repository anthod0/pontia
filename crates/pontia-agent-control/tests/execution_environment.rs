use pontia_agent_control::{
    AgentControl, CreateSessionRequest, EventIngestService, PontiaEvent, PontiaEventSource,
    PontiaEventType,
    client_contract::{
        ClientExitMode, ClientExitOutcome, ClientLaunchRequest, ClientOperation, ClientSession,
        ClientSessionDetails, DeliveryError, DeliveryOperation, InputPayload, test_registration,
    },
    clients::ClientRegistry,
    control::InputReceipt,
    runtime::ControlTarget,
};
use pontia_core::Result;
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Launcher(Mutex<Vec<RuntimeStartRequest>>);
impl pontia_agent_control::client_contract::ClientConfiguration for Launcher {
    fn supports_mode(&self, _: &str, _: pontia_core::SessionMode) -> bool {
        true
    }
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, _: &str, _: &serde_json::Value) -> (String, String) {
        ("Test".into(), "generic".into())
    }
    fn key(&self, _: &serde_json::Value) -> Result<String> {
        Ok("default".into())
    }
    fn merged(&self, _: &str) -> serde_json::Value {
        json!({})
    }
    fn resolve(&self, _: &str) -> Result<serde_json::Value> {
        Ok(json!({}))
    }
}
impl Launcher {
    fn launch(&self, request: ClientLaunchRequest<'_>) -> Result<RuntimeStartResult> {
        self.0.lock().unwrap().push(request.runtime.clone());
        GenericRuntimeManager.start_in_process(
            request.root,
            request.runtime,
            Default::default(),
            request.restart_count,
        )
    }
}
impl ClientSession for Launcher {
    fn provision<'a>(
        &'a self,
        _events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move {
            self.launch(ClientLaunchRequest {
                root,
                runtime: request,
                restart_count: 0,
                reuse_pane: None,
                native_session_key: None,
                native_session_file: None,
            })
            .map(Some)
        })
    }
    fn resume_with_runtime<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        request: ClientLaunchRequest<'a>,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move { self.launch(request).map(Some) })
    }
    fn start_turn<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _payload: &'a InputPayload,
        _turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async { Err(DeliveryError::Unsupported("unused".into())) })
    }
    fn exit<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async { Ok(ClientExitOutcome::Requested) })
    }
    fn available<'a>(
        &'a self,
        _pool: sqlx::SqlitePool,
        _session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn details<'a>(
        &'a self,
        _pool: sqlx::SqlitePool,
        _session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async { Ok(ClientSessionDetails { data: json!({}) }) })
    }
}
#[tokio::test]
async fn creation_restart_and_recovered_resume_keep_launch_context_out_of_events() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        pool.clone(),
    )
    .save("generic", "default", true)
    .await
    .unwrap();
    let launcher = Arc::new(Launcher::default());
    let mut client = test_registration();
    client.session = Some(launcher.clone());
    client.configuration = Some(launcher.clone());
    let mut clients = ClientRegistry::default();
    clients.register(client);
    let environment = BTreeMap::from([
        ("CALLER_CONTEXT".to_string(), "context-1".to_string()),
        ("CALLER_EXTENSION".to_string(), "extension-2".to_string()),
        (
            "API_KEY".to_string(),
            "fixture-secret-must-not-leak".to_string(),
        ),
    ]);
    let build = || {
        AgentControl::builder(pool.clone(), root.path().into())
            .clients(clients.clone())
            .build()
    };
    let control = build();
    let mut notices = control.agent_events().subscribe();
    let mut request: CreateSessionRequest = serde_json::from_value(json!({
        "client_type": "generic", "workspace": root.path(),
        "mode":"restricted"
    }))
    .unwrap();
    assert_eq!(request.launch_mode, pontia_core::LaunchMode::Foreground);
    request.launch_mode = pontia_core::LaunchMode::Background;
    request.runtime_environment = environment.clone();
    let mode = request.mode;
    let outcome = control
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = outcome.session_id().unwrap();
    let created = notices.try_recv().unwrap();
    assert_eq!(created.event_type, PontiaEventType::SessionCreated.into());
    assert_eq!(created.payload["launch_mode"], "background");
    assert!(created.payload.get("runtime_environment").is_none());
    assert!(
        !created
            .payload
            .to_string()
            .contains("fixture-secret-must-not-leak")
    );
    control
        .session_commands()
        .restart_session(session, root.path())
        .await
        .unwrap();
    control
        .event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            session,
            None,
            PontiaEventSource::RuntimeManager,
            "generic",
            PontiaEventType::SessionExited,
            json!({}),
        ))
        .await
        .unwrap();
    let recovered = build();
    recovered
        .session_commands()
        .resume_session(session, root.path())
        .await
        .unwrap();
    {
        let launches = launcher.0.lock().unwrap();
        assert_eq!(launches.len(), 3);
        for launch in launches.iter() {
            assert_eq!(launch.session_id, session);
            assert_eq!(launch.launch_mode, pontia_core::LaunchMode::Background);
            assert_eq!(launch.environment, environment);
            assert_eq!(launch.mode, mode);
            assert_eq!(launch.workspace.as_deref(), root.path().to_str());
        }
    }
    let events = recovered
        .event_ingest_service()
        .list_events(session)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.event_id == created.event_id)
    );
    for event in events {
        assert!(event.payload.get("runtime_environment").is_none());
        assert!(
            !event
                .payload
                .to_string()
                .contains("fixture-secret-must-not-leak")
        );
    }
}

#[test]
fn create_accepts_only_named_session_modes() {
    for mode in ["default", "yolo", "restricted"] {
        let request: CreateSessionRequest = serde_json::from_value(json!({"mode":mode})).unwrap();
        assert_eq!(serde_json::to_value(request.mode).unwrap(), mode);
    }
    for invalid in [
        json!({"mode":"background"}),
        json!({"execution_policy":{"yolo":true}}),
    ] {
        assert!(serde_json::from_value::<CreateSessionRequest>(invalid).is_err());
    }
}

#[tokio::test]
async fn unsupported_mode_is_rejected_before_creating_a_session() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(test_registration());
    let app = AgentControl::builder(pool.clone(), root.path().into())
        .clients(clients)
        .build();
    for mode in ["yolo", "restricted"] {
        let request = serde_json::from_value(
            json!({"client_type":"generic", "workspace":root.path(), "mode":mode}),
        )
        .unwrap();
        assert!(matches!(
            app.session_commands().create_session(request).await,
            Err(pontia_core::Error::CapabilityUnavailable(_))
        ));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn native_forks_inherit_the_source_mode_after_control_recovery() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let app = AgentControl::builder(pool.clone(), root.path().into()).build();
    let mode = json!("restricted");
    for (session, payload) in [
        ("parent", json!({"mode":mode,"launch_mode":"background"})),
        ("legacy", json!({})),
        ("child", json!({})),
        ("grandchild", json!({})),
    ] {
        app.event_ingest_service()
            .ingest_pontia_event(PontiaEvent::new(
                session,
                None,
                PontiaEventSource::RuntimeManager,
                "generic",
                PontiaEventType::SessionCreated,
                payload,
            ))
            .await
            .unwrap();
    }
    for (child, parent) in [("child", "parent"), ("grandchild", "child")] {
        sqlx::query("INSERT INTO session_lineage(child_session_id,parent_session_id,relation_type) VALUES (?,?,'fork')")
            .bind(child).bind(parent).execute(&pool).await.unwrap();
    }
    let recovered = AgentControl::builder(pool, root.path().into()).build();
    assert_eq!(
        recovered
            .queries()
            .session_launch_mode("legacy")
            .await
            .unwrap(),
        pontia_core::LaunchMode::Foreground
    );
    assert_eq!(
        recovered
            .queries()
            .session_launch_mode("grandchild")
            .await
            .unwrap(),
        pontia_core::LaunchMode::Background
    );
    assert_eq!(
        serde_json::to_value(
            recovered
                .queries()
                .session_mode("grandchild")
                .await
                .unwrap()
        )
        .unwrap(),
        mode
    );
}

#[tokio::test]
async fn owned_creation_is_idempotent_and_keeps_the_original_environment() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("owned.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        pool.clone(),
    )
    .save("generic", "default", true)
    .await
    .unwrap();
    let launcher = Arc::new(Launcher::default());
    let mut client = test_registration();
    client.session = Some(launcher.clone());
    let mut clients = ClientRegistry::default();
    clients.register(client);
    let app = AgentControl::builder(pool, root.path().into())
        .clients(clients)
        .build();
    let ownership = pontia_agent_control::SessionOwnership {
        application_id: "example".into(),
        reference: "task:one".into(),
    };
    let mut request: CreateSessionRequest =
        serde_json::from_value(json!({"client_type":"generic","workspace":root.path()})).unwrap();
    request.ownership = Some(ownership.clone());
    request.runtime_environment = BTreeMap::from([("TASK_ID".into(), "one".into())]);
    let service = app.session_commands();
    let (first, second) = tokio::join!(
        service.create_session(request.clone()),
        service.create_session(request)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.session_id(), second.session_id());
    assert_ne!(first.duplicate, second.duplicate);
    assert_eq!(launcher.0.lock().unwrap().len(), 1);
    assert_eq!(
        app.queries()
            .find_session_by_ownership(&ownership)
            .await
            .unwrap()
            .as_deref(),
        first.session_id()
    );
    assert_eq!(
        app.queries().list_owned_sessions("example").await.unwrap(),
        vec![first.session_id().unwrap()]
    );
}
