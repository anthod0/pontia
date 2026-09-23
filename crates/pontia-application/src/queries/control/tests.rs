use crate::{
    AppState, EventIngestService,
    client_contract::{ClientOperation, ClientSession, ClientSessionDetails},
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
}
impl ClientSession for Client {
    fn input<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a str,
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
    fn exit<'a>(&'a self, _: EventIngestService, _: &'a ControlTarget) -> ClientOperation<'a, ()> {
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
    fn open_interface<'a>(&'a self, _: EventIngestService, _: &'a str) -> ClientOperation<'a, ()> {
        panic!("unexpected interface launch")
    }
}

#[tokio::test]
async fn input_checks_persisted_capabilities_without_fetching_display_details() -> Result<()> {
    let root = tempfile::tempdir()?;
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:").await?;
    pontia_storage_sqlite::run_migrations(&db).await?;
    let client = Arc::new(Client {
        inputs: AtomicUsize::new(0),
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
    sqlx::query("INSERT INTO runtime_bindings(session_id,runtime_kind,runtime_instance_id,binding_state,capabilities) VALUES ('session','test','runtime','confirmed','{\"accept_task\":true}')").execute(&db).await?;
    assert!(app.queries().get_session("session").await.is_err());
    assert!(
        app.turn_commands()
            .create_and_dispatch_turn("session", "input".into(), json!({}))
            .await?
            .is_none()
    );
    assert_eq!(client.inputs.load(Ordering::SeqCst), 1);
    sqlx::query("UPDATE runtime_bindings SET capabilities='{}' WHERE session_id='session'")
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
