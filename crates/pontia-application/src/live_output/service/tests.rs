use crate::{
    AppState, LiveOutputBatch, LiveOutputIdentity, LiveOutputProducer, LiveOutputPublishOutcome,
    LiveOutputSnapshotReplacement, LiveOutputSource, LiveOutputUpdate,
    client_contract::{
        AgentClientCapabilities, AgentClientSpec, RuntimeBindingBehavior, TEST_SPEC,
        test_registration,
    },
    clients::ClientRegistry,
};
use pontia_core::Error;

const RUNTIME_SPEC: AgentClientSpec = AgentClientSpec {
    adapter: crate::client_contract::AgentClientAdapter {
        runtime_binding: RuntimeBindingBehavior::Named {
            runtime_kind: "test",
        },
        ..TEST_SPEC.adapter
    },
    capabilities: AgentClientCapabilities {
        stream_output: true,
        ..TEST_SPEC.capabilities
    },
    ..TEST_SPEC
};
const SHARED_SPEC: AgentClientSpec = AgentClientSpec {
    adapter: crate::client_contract::AgentClientAdapter {
        runtime_binding: RuntimeBindingBehavior::SharedBackend,
        ..TEST_SPEC.adapter
    },
    ..RUNTIME_SPEC
};

async fn state(spec: &'static AgentClientSpec) -> AppState {
    let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let mut registration = test_registration();
    registration.spec = spec;
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let app = AppState::builder(pool, "/unused".into())
        .clients(clients)
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','busy'),('other','generic','busy')")
        .execute(&app.db()).await.unwrap();
    sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','session','running'),('foreign','other','running')")
        .execute(&app.db()).await.unwrap();
    app
}

fn producer(source: LiveOutputSource) -> LiveOutputProducer {
    LiveOutputProducer {
        identity: LiveOutputIdentity {
            session_id: "session".into(),
            turn_id: "turn".into(),
            stream_id: "stream".into(),
        },
        source,
    }
}

async fn snapshot(
    app: &AppState,
    producer: LiveOutputProducer,
) -> pontia_core::Result<LiveOutputPublishOutcome> {
    app.live_output()
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer,
            sequence: 1,
            items: Vec::new(),
        })
        .await
}

#[tokio::test]
async fn shared_backend_streams_without_a_session_runtime_and_rejects_runtime_sources() {
    let app = state(&SHARED_SPEC).await;
    assert!(matches!(
        snapshot(
            &app,
            producer(LiveOutputSource::RuntimeBound {
                runtime_id: "invented".into()
            })
        )
        .await,
        Err(Error::StateConflict(_))
    ));
    let shared = producer(LiveOutputSource::SharedBackend);
    assert_eq!(
        snapshot(&app, shared.clone()).await.unwrap(),
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 1,
            duplicate: false
        }
    );
    let batch = LiveOutputBatch {
        producer: shared,
        first_sequence: 2,
        updates: vec![LiveOutputUpdate::AssistantTextDelta {
            item_id: "item".into(),
            delta: "hello".into(),
        }],
    };
    app.live_output()
        .publish_batch(batch.clone())
        .await
        .unwrap();
    assert_eq!(
        app.live_output().publish_batch(batch).await.unwrap(),
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 2,
            duplicate: true
        }
    );
    let snapshot = app
        .live_output()
        .subscribe_session("session")
        .initial_snapshot
        .unwrap();
    assert_eq!(snapshot.sequence, 2);
    assert_eq!(
        snapshot.items,
        vec![crate::LiveOutputItem::AssistantText {
            item_id: "item".into(),
            text: "hello".into()
        }]
    );
    let runtimes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes")
        .fetch_one(&app.db())
        .await
        .unwrap();
    assert_eq!(runtimes, 0);
}

#[tokio::test]
async fn runtime_bound_streaming_requires_the_current_session_runtime() {
    let app = state(&RUNTIME_SPEC).await;
    assert!(matches!(
        snapshot(&app, producer(LiveOutputSource::SharedBackend)).await,
        Err(Error::StateConflict(_))
    ));
    assert!(
        snapshot(
            &app,
            producer(LiveOutputSource::RuntimeBound {
                runtime_id: "runtime".into()
            })
        )
        .await
        .is_err()
    );
    sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','tui','running','2026-10-01T00:00:00Z')").execute(&app.db()).await.unwrap();
    snapshot(
        &app,
        producer(LiveOutputSource::RuntimeBound {
            runtime_id: "runtime".into(),
        }),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id='session'")
        .execute(&app.db())
        .await
        .unwrap();
    assert!(matches!(
        snapshot(
            &app,
            producer(LiveOutputSource::RuntimeBound {
                runtime_id: "runtime".into()
            })
        )
        .await,
        Err(Error::StateConflict(_))
    ));
}

#[tokio::test]
async fn shared_backend_validates_session_capability_turn_ownership_and_activity() {
    let app = state(&SHARED_SPEC).await;
    for (session, turn) in [
        ("missing", "turn"),
        ("session", "missing"),
        ("session", "foreign"),
    ] {
        let mut invalid = producer(LiveOutputSource::SharedBackend);
        invalid.identity.session_id = session.into();
        invalid.identity.turn_id = turn.into();
        assert!(snapshot(&app, invalid).await.is_err());
    }
    for terminal in [
        "completed",
        "failed",
        "interrupted",
        "abandoned",
        "dispatch_failed",
    ] {
        sqlx::query("UPDATE turns SET state=? WHERE turn_id='turn'")
            .bind(terminal)
            .execute(&app.db())
            .await
            .unwrap();
        assert!(
            snapshot(&app, producer(LiveOutputSource::SharedBackend))
                .await
                .is_err()
        );
        assert!(app.live_output().snapshot("session", "turn").is_none());
    }
    let unsupported = state(&TEST_SPEC).await;
    assert!(matches!(
        snapshot(&unsupported, producer(LiveOutputSource::SharedBackend)).await,
        Err(Error::CapabilityUnavailable(_))
    ));
}
