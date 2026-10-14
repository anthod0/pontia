use crate::{
    AppState, EventIngestService,
    client_contract::{ClientExitOutcome, ClientOperation, ClientSession, ClientSessionDetails},
    control::InputReceipt,
    runtime::ControlTarget,
    sessions::SessionModel,
    turns::InputIntent,
};
use pontia_core::{Error, Result};
use pontia_runtime::RuntimeStartRequest;
use serde_json::json;
use sqlx::SqlitePool;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Client {
    inputs: AtomicUsize,
    capabilities: Option<crate::client_contract::AgentClientCapabilities>,
}
impl ClientSession for Client {
    fn capabilities<'a>(
        &'a self,
        _: &'a str,
        implementation: &'a crate::client_contract::AgentClientCapabilities,
    ) -> ClientOperation<'a, crate::client_contract::AgentClientCapabilities> {
        Box::pin(async move { Ok(self.capabilities.as_ref().unwrap_or(implementation).clone()) })
    }
    fn input<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a str,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt> {
        self.inputs.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(InputReceipt::default()) })
    }
    fn details<'a>(
        &'a self,
        _: SqlitePool,
        _: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async { Err(Error::Domain("display details unavailable".into())) })
    }
    fn available<'a>(&'a self, _: SqlitePool, _: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn provision<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a Path,
        _: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()> {
        panic!("unexpected provisioning")
    }
    fn interrupt<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a str,
    ) -> ClientOperation<'a, ()> {
        panic!("unexpected interrupt")
    }
    fn exit<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        panic!("unexpected exit")
    }
    fn resume<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        panic!("unexpected resume")
    }
    fn list_models<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
    ) -> ClientOperation<'a, Vec<SessionModel>> {
        panic!("unexpected model query")
    }
    fn set_model<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a str,
    ) -> ClientOperation<'a, ()> {
        panic!("unexpected model change")
    }
    fn open_interface<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a Path,
        _: &'a str,
        _: &'a str,
    ) -> ClientOperation<'a, pontia_runtime::RuntimeStartResult> {
        panic!("unexpected interface launch")
    }
}

#[tokio::test]
async fn session_resolver_can_narrow_but_not_expand_static_implementation_capabilities()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:").await?;
    pontia_storage_sqlite::run_migrations(&db).await?;
    let mut registration = crate::client_contract::test_registration();
    registration.in_process = None;
    registration.session = Some(Arc::new(Client {
        inputs: AtomicUsize::new(0),
        capabilities: Some(crate::client_contract::AgentClientCapabilities {
            accept_task: false,
            report_turn_started: true,
            report_turn_finished: true,
            interrupt: true,
            timeline: true,
            context_usage: crate::client_contract::ContextUsageCapability::Exact,
            ..Default::default()
        }),
    }));
    let mut clients = crate::clients::ClientRegistry::default();
    clients.register(registration);
    let app = AppState::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','idle')",
    )
    .execute(&db)
    .await?;
    sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','generic','running','2026-01-01T00:00:00Z')").execute(&db).await?;
    let capabilities = app
        .queries()
        .get_session_control("session")
        .await?
        .unwrap()
        .capabilities;
    assert!(!capabilities.accept_task);
    assert!(!capabilities.interrupt);
    assert!(!capabilities.timeline);
    assert_eq!(
        capabilities.context_usage,
        crate::client_contract::ContextUsageCapability::Unsupported
    );
    Ok(())
}

#[tokio::test]
async fn input_uses_client_capabilities_without_fetching_display_details() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:").await?;
    pontia_storage_sqlite::run_migrations(&db).await?;
    let client = Arc::new(Client {
        inputs: AtomicUsize::new(0),
        capabilities: None,
    });
    let mut registration = crate::client_contract::test_registration();
    registration.session = Some(client.clone());
    registration.prepare_on_input = true;
    let mut clients = crate::clients::ClientRegistry::default();
    clients.register(registration);
    let app = AppState::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','idle')",
    )
    .execute(&db)
    .await?;
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, role, state, created_at) VALUES ('session', 'runtime', 'tui', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&db).await?;
    assert!(app.queries().get_session("session").await.is_err());
    assert!(
        app.turn_commands()
            .create_and_dispatch_turn("session", "input".into(), json!({}))
            .await?
            .is_none()
    );
    assert_eq!(client.inputs.load(Ordering::SeqCst), 1);
    sqlx::query("DELETE FROM session_runtimes WHERE session_id='session'")
        .execute(&db)
        .await?;
    assert!(matches!(
        app.turn_commands()
            .create_and_dispatch_turn("session", "blocked".into(), json!({}))
            .await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(client.inputs.load(Ordering::SeqCst), 1);
    Ok(())
}
