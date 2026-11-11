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
async fn replanner_restart_and_recovered_resume_keep_the_original_patch_context() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("sessions.db").display()
    ))
    .await
    .unwrap();
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
        ("PONTIA_WORKFLOW_ID".to_string(), "wf_original".to_string()),
        (
            "PONTIA_WORKFLOW_PATCH_ID".to_string(),
            "patch_original".to_string(),
        ),
    ]);
    let build = || {
        AgentControl::builder(pool.clone(), root.path().into())
            .clients(clients.clone())
            .environment(Arc::new(pontia_workflow::WorkflowSessionEnvironment::new(
                pool.clone(),
            )))
            .build()
    };
    let control = build();
    let mut request: CreateSessionRequest = serde_json::from_value(json!({
        "client_type": "generic", "workspace": root.path(),
        "role": "workflow_replanner",
        "metadata": {
            "role": "workflow_replanner",
            "workflow_id": "wf_original",
            "workflow_patch_id": "patch_original",
            "workflow_replanner_creation_token": "original-creation"
        }
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
    let mut later: CreateSessionRequest = serde_json::from_value(json!({
        "client_type": "generic", "workspace": root.path(),
        "role": "workflow_replanner",
        "metadata": {
            "role": "workflow_replanner",
            "workflow_id": "wf_original",
            "workflow_patch_id": "patch_later",
            "workflow_replanner_creation_token": "later-creation"
        }
    }))
    .unwrap();
    later.runtime_environment = BTreeMap::from([
        ("PONTIA_WORKFLOW_ID".into(), "wf_original".into()),
        ("PONTIA_WORKFLOW_PATCH_ID".into(), "patch_later".into()),
    ]);
    control
        .session_commands()
        .create_session(later)
        .await
        .unwrap();
    drop(control);
    let recovered = build();
    recovered
        .session_commands()
        .resume_session(session, root.path())
        .await
        .unwrap();
    let launches = launcher.0.lock().unwrap();
    let original: Vec<_> = launches
        .iter()
        .filter(|launch| launch.session_id == session)
        .collect();
    assert_eq!(original.len(), 3);
    for launch in original {
        assert_eq!(launch.environment, environment);
        assert_eq!(launch.workspace.as_deref(), root.path().to_str());
    }
}
