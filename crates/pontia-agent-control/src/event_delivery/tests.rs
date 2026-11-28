use super::*;
use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use std::{collections::BTreeSet, time::Duration};
use tokio::{
    sync::{Notify, mpsc, watch},
    time::{Instant, timeout},
};

struct Fixture {
    app: crate::AgentControl,
    home: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&db).await.unwrap();
        let home = tempfile::tempdir().unwrap();
        let app = crate::AgentControl::builder(db, home.path().into()).build();
        Self { app, home }
    }

    async fn create(&self, id: &str) -> Result<()> {
        self.app
            .event_ingest_service()
            .ingest_reported_event(ReportedEvent::new(
                id.into(),
                id.into(),
                None,
                EventSource::ExternalApi,
                "generic".into(),
                EventType::SessionCreated,
                serde_json::json!({}),
            ))
            .await?;
        Ok(())
    }

    fn restart(&self) -> EventDeliveryService {
        crate::AgentControl::builder(self.app.db(), self.home.path().into())
            .build()
            .events()
    }
}

#[tokio::test]
async fn tail_wakes_after_commit_and_rollbacks_and_duplicates_leave_head_unchanged() {
    let f = Fixture::new().await;
    let service = f.app.events();
    let mut watermark = service.broker.head_position.subscribe();
    f.create("seed").await.unwrap();
    let mut tail = service.tail(EventScope::All, 0, false);
    assert_eq!(tail.next().await.unwrap().unwrap().event_id, "seed");
    assert_eq!(*watermark.borrow_and_update(), 1);
    assert!(
        timeout(Duration::from_millis(20), tail.next())
            .await
            .is_err()
    );
    sqlx::query("CREATE TRIGGER fail_creation BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT,'rollback'); END").execute(&f.app.db()).await.unwrap();
    assert!(f.create("s").await.is_err());
    assert_eq!(service.current_head_position().await.unwrap(), 1);
    assert!(!watermark.has_changed().unwrap());
    sqlx::query("DROP TRIGGER fail_creation")
        .execute(&f.app.db())
        .await
        .unwrap();
    f.create("other").await.unwrap();
    f.create("s").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), tail.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .position,
        2
    );
    assert_eq!(tail.next().await.unwrap().unwrap().position, 3);
    assert_eq!(*watermark.borrow_and_update(), 3);
    f.create("s").await.unwrap();
    assert!(!watermark.has_changed().unwrap());
    let mut once = f.restart().tail(EventScope::Session("s".into()), 1, true);
    let replay = once.next().await.unwrap().unwrap();
    assert_eq!(replay.event_id, "s");
    let session_events = f.app.queries().list_session_events("s").await.unwrap();
    assert_eq!(
        crate::EventView::try_from(replay).unwrap(),
        session_events[0]
    );
    assert!(once.next().await.unwrap().is_none());
}

#[tokio::test]
async fn startup_catchup_skips_empty_windows_and_rejects_duplicate_registration() {
    let f = Fixture::new().await;
    for id in ["a", "b", "c"] {
        f.create(id).await.unwrap();
    }
    let service = f.restart();
    let mut config = SubscriptionConfig::new("filtered");
    config.scope = EventScope::Session("b".into());
    config.scan_window = 1.try_into().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let subscription = service
        .register_durable(config.clone(), move |event| {
            tx.send(event.event_id).unwrap();
            async { Ok(()) }
        })
        .await
        .unwrap();
    assert!(
        service
            .clone()
            .register_durable(config, |_| async { Ok(()) })
            .await
            .is_err()
    );
    subscription.catch_up().await.unwrap();
    assert_eq!(rx.try_recv().unwrap(), "b");
    assert!(rx.try_recv().is_err());
    assert_eq!(service.store.cursor("filtered").await.unwrap(), 3);
}

#[tokio::test]
async fn failure_backoff_survives_commit_wakeups_and_shutdown_interrupts_retry() {
    let f = Fixture::new().await;
    f.create("first").await.unwrap();
    let service = f.app.events();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sub = service
        .register_durable(SubscriptionConfig::new("retry"), move |event| {
            tx.send((Instant::now(), event.position)).unwrap();
            async { Err(Error::Domain("busy".into())) }
        })
        .await
        .unwrap();
    let (stop, shutdown) = watch::channel(false);
    let task = tokio::spawn(async move { sub.run(shutdown).await });
    let (first, pos) = rx.recv().await.unwrap();
    assert_eq!(pos, 1);
    f.create("later").await.unwrap();
    let (second, pos) = timeout(Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pos, 1);
    assert!(second.duration_since(first) >= Duration::from_millis(250));
    let (third, pos) = timeout(Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pos, 1);
    assert!(third.duration_since(second) >= Duration::from_millis(500));
    stop.send_replace(true);
    timeout(Duration::from_millis(500), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(service.store.cursor("retry").await.unwrap(), 0);
}

#[tokio::test]
async fn active_delivery_finishes_before_shutdown_and_catchup_resumes_at_ack() {
    let f = Fixture::new().await;
    for id in ["one", "two"] {
        f.create(id).await.unwrap();
    }
    let service = f.app.events();
    let release = Arc::new(Notify::new());
    let gate = release.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sub = Arc::new(
        service
            .register_durable(SubscriptionConfig::new("serial"), move |event| {
                let gate = gate.clone();
                tx.send(event.position).unwrap();
                async move {
                    if event.position == 1 {
                        gate.notified().await;
                    }
                    Ok(())
                }
            })
            .await
            .unwrap(),
    );
    let (stop, shutdown) = watch::channel(false);
    let runner = sub.clone();
    let task = tokio::spawn(async move { runner.run(shutdown).await });
    assert_eq!(rx.recv().await.unwrap(), 1);
    let mut catchup = Box::pin(sub.catch_up());
    assert!(
        timeout(Duration::from_millis(30), &mut catchup)
            .await
            .is_err()
    );
    assert!(rx.try_recv().is_err());
    stop.send_replace(true);
    release.notify_one();
    task.await.unwrap().unwrap();
    assert_eq!(service.store.cursor("serial").await.unwrap(), 1);
    catchup.await.unwrap();
    assert_eq!(rx.try_recv().unwrap(), 2);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn restart_replays_successful_handler_when_ack_failed() {
    let f = Fixture::new().await;
    for id in ["one", "two"] {
        f.create(id).await.unwrap();
    }
    let applied = Arc::new(Mutex::new(BTreeSet::new()));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let handler = move |event: PositionedEvent| {
        let inserted = applied.lock().unwrap().insert(event.event_id);
        tx.send((event.position, inserted)).unwrap();
        async { Ok(()) }
    };
    let config = SubscriptionConfig::new("recover");
    let service = f.app.events();
    let sub = service
        .register_durable(config.clone(), handler.clone())
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fail_ack BEFORE UPDATE ON event_subscriptions WHEN NEW.cursor_position=2 BEGIN SELECT RAISE(ABORT,'lost acknowledgement'); END")
        .execute(&f.app.db()).await.unwrap();
    assert!(sub.catch_up().await.is_err());
    assert_eq!(service.store.cursor("recover").await.unwrap(), 1);
    sqlx::query("DROP TRIGGER fail_ack")
        .execute(&f.app.db())
        .await
        .unwrap();
    let resumed = f.restart().register_durable(config, handler).await.unwrap();
    resumed.catch_up().await.unwrap();
    assert_eq!(rx.try_recv().unwrap(), (1, true));
    assert_eq!(rx.try_recv().unwrap(), (2, true));
    assert_eq!(rx.try_recv().unwrap(), (2, false));
    assert!(rx.try_recv().is_err());
    assert_eq!(service.store.cursor("recover").await.unwrap(), 2);
}
