use std::{path::Path, sync::Arc, time::Duration};

use pontia_application::{
    AppState, EventIngestService,
    client_contract::{ClientExitOutcome, ClientOperation, ClientSession, ClientSessionDetails},
    clients::ClientRegistry,
    control::InputReceipt,
    runtime::ControlTarget,
    sessions::SessionModel,
    turns::InputIntent,
};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::SqlitePool;

struct ManagedTuiClient;

impl ClientSession for ManagedTuiClient {
    fn provision<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a Path,
        _: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()> {
        panic!("unexpected provisioning")
    }

    fn input<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a str,
        _: Option<&'a str>,
        _: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt> {
        panic!("unexpected input")
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
        panic!("unexpected model update")
    }

    fn available<'a>(&'a self, _: SqlitePool, _: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async { Ok(true) })
    }

    fn open_interface<'a>(
        &'a self,
        _: EventIngestService,
        root: &'a Path,
        session: &'a str,
        runtime_id: &'a str,
    ) -> ClientOperation<'a, RuntimeStartResult> {
        Box::pin(async move {
            let mut result = GenericRuntimeManager.start_tmux(
                root,
                RuntimeStartRequest {
                    session_id: session.into(),
                    runtime_id: Some(runtime_id.into()),
                    client_type: "codex".into(),
                    workspace: Some(root.display().to_string()),
                    workspace_name: None,
                    handle: None,
                    role: Some("interface".into()),
                    start_command: Some("exec sleep 60".into()),
                    environment: Default::default(),
                },
                0,
                None,
                &pontia_runtime::TmuxLaunchOptions {
                    capabilities:
                        pontia_application::client_contract::AgentClientCapabilities::generic_default(),
                    hook_log: None,
                },
            )?;
            let socket = result.tmux_socket_path().expect("tmux socket").to_string();
            let pane = result.tmux_pane_id().expect("tmux pane").to_string();
            let fingerprint = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(fingerprint) = GenericRuntimeManager
                        .capture_tmux_process_fingerprint(&socket, &pane, &["sleep"])
                    {
                        break fingerprint;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .map_err(|_| Error::ControlUnknown("test TUI did not start".into()))?;
            result.metadata["tmux_process_fingerprint"] = serde_json::to_value(fingerprint)?;
            Ok(result)
        })
    }

    fn details<'a>(
        &'a self,
        _: SqlitePool,
        _: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async {
            Ok(ClientSessionDetails {
                data: serde_json::json!({}),
                model_control_unavailable_reason: None,
            })
        })
    }
}

#[tokio::test]
async fn managed_codex_tui_has_one_restartable_runtime_without_changing_the_session() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let db = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await?;
    run_migrations(&db).await?;
    let mut registration = pontia_application::client_contract::test_registration();
    static SPEC: pontia_application::client_contract::AgentClientSpec =
        pontia_application::client_contract::AgentClientSpec {
            client_type: "codex",
            ..pontia_application::client_contract::TEST_SPEC
        };
    registration.spec = &SPEC;
    registration.in_process = None;
    registration.session = Some(Arc::new(ManagedTuiClient));
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let app = AppState::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query(
        "INSERT INTO sessions(session_id, client_type, state) VALUES ('session', 'codex', 'idle')",
    )
    .execute(&db)
    .await?;

    app.session_commands().start_tui("session").await?;
    let first_runtime: (String, String, String) = sqlx::query_as(
        "SELECT runtime_id, role, state FROM session_runtimes WHERE session_id = 'session'",
    )
    .fetch_one(&db)
    .await?;
    assert_eq!(first_runtime.1, "interface");
    assert_eq!(first_runtime.2, "running");
    assert!(matches!(
        app.session_commands().start_tui("session").await,
        Err(Error::StateConflict(_))
    ));

    app.session_commands().stop_tui("session").await?;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM session_runtimes WHERE runtime_id = ?")
            .bind(&first_runtime.0)
            .fetch_one(&db)
            .await?,
        "exited"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM sessions WHERE session_id = 'session'")
            .fetch_one(&db)
            .await?,
        "idle"
    );

    app.session_commands().start_tui("session").await?;
    let restarted_runtime: (String, String) = sqlx::query_as(
        "SELECT runtime_id, state FROM session_runtimes WHERE session_id = 'session'",
    )
    .fetch_one(&db)
    .await?;
    assert_eq!(restarted_runtime, (first_runtime.0, "running".into()));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM session_runtimes WHERE session_id = 'session'"
        )
        .fetch_one(&db)
        .await?,
        1
    );

    app.session_commands().stop_tui("session").await?;
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM events WHERE session_id = 'session' ORDER BY rowid",
    )
    .fetch_all(&db)
    .await?;
    assert_eq!(
        events,
        vec![
            "runtime.starting",
            "runtime.ready",
            "runtime.exited",
            "runtime.starting",
            "runtime.ready",
            "runtime.exited",
        ]
    );
    Ok(())
}
