use pontia_agent_control::{
    AgentControl, CreateSessionRequest, PontiaEvent, PontiaEventSource, PontiaEventType,
    client_contract::{ClientLaunchRequest, ClientLauncher, ClientOperation, test_registration},
    clients::ClientRegistry,
    sessions::SessionEnvironment,
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
struct Environment(BTreeMap<String, String>);
impl SessionEnvironment for Environment {
    fn environment<'a>(&'a self, _: &'a str) -> ClientOperation<'a, BTreeMap<String, String>> {
        Box::pin(async { Ok(self.0.clone()) })
    }
}

#[tokio::test]
async fn creation_restart_and_recovered_resume_keep_caller_launch_context() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
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
    let environment = BTreeMap::from([
        ("CALLER_CONTEXT".to_string(), "context-1".to_string()),
        ("CALLER_EXTENSION".to_string(), "extension-2".to_string()),
    ]);
    let build = || {
        AgentControl::builder(pool.clone(), root.path().into())
            .clients(clients.clone())
            .environment(Arc::new(Environment(environment.clone())))
            .build()
    };
    let control = build();
    let mut request: CreateSessionRequest = serde_json::from_value(json!({
        "client_type": "generic", "workspace": root.path()
    }))
    .unwrap();
    request.runtime_environment = environment.clone();
    let outcome = control
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = outcome.session_id().unwrap();
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
    let launches = launcher.0.lock().unwrap();
    assert_eq!(launches.len(), 3);
    for launch in launches.iter() {
        assert_eq!(launch.session_id, session);
        assert_eq!(launch.environment, environment);
        assert_eq!(launch.workspace.as_deref(), root.path().to_str());
    }
}
