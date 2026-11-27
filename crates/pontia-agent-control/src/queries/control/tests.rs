use crate::{
    AgentControl, EventIngestService,
    client_contract::{
        ClientExitMode, ClientExitOutcome, ClientOperation, ClientSession, ClientSessionDetails,
        DeliveryOperation, InputPayload,
    },
    control::InputReceipt,
    runtime::ControlTarget,
};
use pontia_core::{Error, Result};
use pontia_runtime::RuntimeStartRequest;
use serde_json::json;
use sqlx::SqlitePool;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Client {
    inputs: AtomicUsize,
    reasoning_efforts: Mutex<Vec<String>>,
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
    fn start_turn<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a InputPayload,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        self.inputs.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(InputReceipt::default()) })
    }
    fn set_reasoning_effort<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        reasoning_effort: &'a str,
    ) -> ClientOperation<'a, ()> {
        self.reasoning_efforts
            .lock()
            .unwrap()
            .push(reasoning_effort.into());
        Box::pin(async { Ok(()) })
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
    ) -> DeliveryOperation<'a> {
        panic!("unexpected interrupt")
    }
    fn exit<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: ClientExitMode,
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
        reasoning_efforts: Mutex::new(Vec::new()),
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
    let app = AgentControl::builder(db.clone(), root.path().into())
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
        reasoning_efforts: Mutex::new(Vec::new()),
        capabilities: None,
    });
    let mut registration = crate::client_contract::test_registration();
    registration.session = Some(client.clone());
    registration.prepare_on_input = true;
    let mut clients = crate::clients::ClientRegistry::default();
    clients.register(registration);
    let app = AgentControl::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','idle')",
    )
    .execute(&db)
    .await?;
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, role, state, created_at) VALUES ('session', 'runtime', 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&db).await?;
    assert!(app.queries().get_session("session").await.is_err());
    assert!(
        app.turn_commands()
            .start_turn("session", "input".into(), json!({}))
            .await?
            .is_none()
    );
    assert_eq!(client.inputs.load(Ordering::SeqCst), 1);
    sqlx::query("DELETE FROM session_runtimes WHERE session_id='session'")
        .execute(&db)
        .await?;
    assert!(matches!(
        app.turn_commands()
            .start_turn("session", "blocked".into(), json!({}))
            .await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(client.inputs.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn reasoning_effort_control_dispatches_without_projecting_unconfirmed_values() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:").await?;
    pontia_storage_sqlite::run_migrations(&db).await?;
    let client = Arc::new(Client {
        inputs: AtomicUsize::new(0),
        reasoning_efforts: Mutex::new(Vec::new()),
        capabilities: None,
    });
    let mut registration = crate::client_contract::test_registration();
    let mut spec = registration.spec.clone();
    spec.capabilities.reasoning_effort_control = true;
    registration.spec = Box::leak(Box::new(spec));
    registration.in_process = None;
    registration.session = Some(client.clone());
    let mut clients = crate::clients::ClientRegistry::default();
    clients.register(registration);
    let app = AgentControl::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state,metadata) VALUES ('session','generic','idle','{\"reasoning_effort\":\"medium\"}')")
        .execute(&db)
        .await?;
    sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','generic_control','running','2026-01-01T00:00:00Z')")
        .execute(&db)
        .await?;

    for (state, reasoning_effort) in [
        ("idle", "high"),
        ("busy", "xhigh"),
        ("waiting_for_interaction", "low"),
    ] {
        sqlx::query("UPDATE sessions SET state=? WHERE session_id='session'")
            .bind(state)
            .execute(&db)
            .await?;
        app.session_commands()
            .set_reasoning_effort(
                "session",
                crate::SetSessionReasoningEffortRequest {
                    reasoning_effort: reasoning_effort.into(),
                },
            )
            .await?;
    }
    assert_eq!(
        client.reasoning_efforts.lock().unwrap().as_slice(),
        ["high", "xhigh", "low"]
    );
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='session'")
        .execute(&db)
        .await?;
    assert!(matches!(
        app.session_commands()
            .set_reasoning_effort(
                "session",
                crate::SetSessionReasoningEffortRequest {
                    reasoning_effort: "max".into(),
                },
            )
            .await,
        Err(Error::StateConflict(_))
    ));
    assert_eq!(
        client.reasoning_efforts.lock().unwrap().as_slice(),
        ["high", "xhigh", "low"]
    );
    let projected: String =
        sqlx::query_scalar("SELECT json_extract(metadata,'$.reasoning_effort') FROM sessions WHERE session_id='session'")
            .fetch_one(&db)
            .await?;
    assert_eq!(projected, "medium");

    sqlx::query("UPDATE sessions SET state='idle' WHERE session_id='session'")
        .execute(&db)
        .await?;
    app.event_ingest_service()
        .report_fact(crate::ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            report_key: None,
            fact_type: pontia_core::domain::EventType::SessionReasoningEffortUpdated,
            data: json!({"runtime_id":"runtime", "reasoning_effort":"high"}),
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?;
    let projected: String =
        sqlx::query_scalar("SELECT json_extract(metadata,'$.reasoning_effort') FROM sessions WHERE session_id='session'")
            .fetch_one(&db)
            .await?;
    assert_eq!(projected, "high");
    Ok(())
}
