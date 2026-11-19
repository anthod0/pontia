use pontia_agent_control::{
    AgentControl, CreateSessionRequest, PontiaEvent, PontiaEventSource, PontiaEventType,
    client_contract::{ClientLaunchRequest, ClientLauncher, test_registration},
    clients::ClientRegistry,
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
    fn validate_execution_policy(
        &self,
        _: &pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<()> {
        Ok(())
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
    fn merged(&self, _: &str, overrides: &serde_json::Value) -> serde_json::Value {
        overrides.clone()
    }
    fn resolve(&self, _: &str, overrides: &serde_json::Value) -> Result<serde_json::Value> {
        Ok(overrides.clone())
    }
}
impl ClientLauncher for Launcher {
    fn validate_resume(&self, _: &str, _: Option<&Path>) -> Result<()> {
        Ok(())
    }
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
    .save("generic", "default", Some(true), None)
    .await
    .unwrap();
    let launcher = Arc::new(Launcher::default());
    let mut client = test_registration();
    client.launcher = Some(launcher.clone());
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
        "execution_policy": {"tools":["read","bash"],"yolo":true}
    }))
    .unwrap();
    request.runtime_environment = environment.clone();
    let policy = request.execution_policy.clone();
    let outcome = control
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = outcome.session_id().unwrap();
    let created = notices.try_recv().unwrap();
    assert_eq!(created.event_type, PontiaEventType::SessionCreated.into());
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
            assert_eq!(launch.environment, environment);
            assert_eq!(launch.execution_policy, policy);
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

#[tokio::test]
async fn unsupported_execution_policy_is_rejected_before_creating_a_session() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(test_registration());
    let app = AgentControl::builder(pool.clone(), root.path().into())
        .clients(clients)
        .build();
    for policy in [
        json!({"tools":["read","bash"]}),
        json!({"yolo":true}),
        json!({"tools":[]}),
    ] {
        let request = serde_json::from_value(
            json!({"client_type":"generic", "workspace":root.path(), "execution_policy":policy}),
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
async fn native_forks_inherit_the_source_policy_after_control_recovery() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let app = AgentControl::builder(pool.clone(), root.path().into()).build();
    let policy = json!({"tools":["read","bash"],"yolo":true});
    for (session, payload) in [
        ("parent", json!({"execution_policy":policy})),
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
        serde_json::to_value(
            recovered
                .queries()
                .session_execution_policy("grandchild")
                .await
                .unwrap()
        )
        .unwrap(),
        policy
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
    .save("generic", "default", Some(true), None)
    .await
    .unwrap();
    let launcher = Arc::new(Launcher::default());
    let mut client = test_registration();
    client.launcher = Some(launcher.clone());
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
