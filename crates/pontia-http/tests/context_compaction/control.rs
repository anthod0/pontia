use super::fixture::*;
use axum::http::StatusCode;
use pontia_core::{
    Error,
    client_capabilities::{AgentClientCapabilities, ContextUsageCapability},
};
use serde_json::json;
use std::sync::atomic::Ordering;
const COMPACT: &str = "/api/v1/sessions/session/compact";

#[test]
fn compaction_capabilities_default_to_false_for_existing_clients() {
    for capabilities in [
        AgentClientCapabilities::default(),
        AgentClientCapabilities::generic_default(),
        serde_json::from_value(json!({"context_usage":"exact"})).unwrap(),
    ] {
        assert!(!capabilities.manual_compaction);
        assert!(!capabilities.compaction_events);
        assert!(!capabilities.compaction_history);
    }
}

#[tokio::test]
async fn accepted_compaction_replays_the_same_operation_without_completing_it() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    for state in ["idle", "interrupted"] {
        fixture.set_state(state).await;
        let (status, first) = fixture.request("POST", COMPACT, Some(state), true).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{first}");
        assert_eq!(first["data"]["accepted"], true);
        let operation = first["data"]["operation_id"].as_str().unwrap();
        assert!(!operation.is_empty());
        let (status, replay) = fixture.request("POST", COMPACT, Some(state), true).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(first, replay);
        let current = fixture
            .state
            .queries()
            .get_session("session")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.state, state);
        assert_eq!(
            fixture.channel.operations.lock().unwrap().last().unwrap(),
            operation
        );
    }
    assert_eq!(fixture.channel.operations.lock().unwrap().len(), 2);
    let events = fixture
        .state
        .queries()
        .list_session_events("session")
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn concurrent_idempotent_requests_trigger_one_compaction() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    fixture.channel.delayed.store(true, Ordering::SeqCst);
    let state = fixture.state.clone();
    let first =
        tokio::spawn(
            async move { request(state, "POST", COMPACT, Some("concurrent"), true).await },
        );
    fixture.channel.started.notified().await;
    let state = fixture.state.clone();
    let second =
        tokio::spawn(
            async move { request(state, "POST", COMPACT, Some("concurrent"), true).await },
        );
    fixture.channel.finish.notify_one();
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    assert_eq!(first.0, StatusCode::ACCEPTED);
    assert_eq!(first, second);
    assert_eq!(fixture.channel.operations.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn compaction_preserves_auth_not_found_capability_and_state_errors() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    assert_eq!(
        fixture.request("POST", COMPACT, None, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = fixture
        .request("POST", "/api/v1/sessions/missing/compact", None, true)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    for state in ["created", "starting", "busy", "exited", "error"] {
        let fixture = Fixture::new(&SPEC, true).await;
        fixture.seed().await;
        fixture.set_state(state).await;
        let error = fixture
            .state
            .session_commands()
            .compact_session("session")
            .await
            .unwrap_err();
        assert!(matches!(error, Error::StateConflict(_)), "{state}: {error}");
        let (status, body) = fixture.request("POST", COMPACT, None, true).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "state_conflict");
    }
    fixture.set_state("idle").await;
    fixture.channel.closed.store(true, Ordering::SeqCst);
    let (status, body) = fixture.request("POST", COMPACT, None, true).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "capability_unavailable");
    assert!(fixture.channel.operations.lock().unwrap().is_empty());
    let unsupported = Fixture::new(&UNSUPPORTED, true).await;
    unsupported.seed().await;
    assert_eq!(
        unsupported.request("POST", COMPACT, None, true).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(unsupported.channel.operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn client_rejection_unknown_and_unsupported_keep_their_api_semantics() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    for (error, status, code) in [
        (
            Error::Conflict {
                code: "control_rejected",
                message: "rejected".into(),
            },
            StatusCode::CONFLICT,
            "control_rejected",
        ),
        (
            Error::ControlUnknown("receipt lost".into()),
            StatusCode::SERVICE_UNAVAILABLE,
            "control_unknown",
        ),
        (
            Error::CapabilityUnavailable("unsupported".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "capability_unavailable",
        ),
    ] {
        *fixture.channel.error.lock().unwrap() = Some(error);
        let (actual, body) = fixture.request("POST", COMPACT, None, true).await;
        assert_eq!(actual, status, "{body}");
        assert_eq!(body["error"]["code"], code);
    }
    assert_eq!(
        fixture
            .state
            .queries()
            .list_session_events("session")
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn replacement_during_compaction_returns_unknown_and_does_not_send_to_replacement() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    fixture.channel.delayed.store(true, Ordering::SeqCst);
    let state = fixture.state.clone();
    let first = tokio::spawn(async move { request(state, "POST", COMPACT, None, true).await });
    fixture.channel.started.notified().await;
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id='session'")
        .execute(&fixture.state.db())
        .await
        .unwrap();
    let replacement = std::sync::Arc::new(Channel::default());
    fixture
        .state
        .client_control()
        .attach(
            SPEC.client_type,
            "session",
            "replacement",
            "native",
            replacement.clone(),
        )
        .await
        .unwrap();
    fixture.channel.finish.notify_one();
    let (status, body) = first.await.unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "control_unknown");
    assert_eq!(fixture.channel.operations.lock().unwrap().len(), 1);
    assert!(replacement.operations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn observation_and_history_capabilities_survive_a_missing_writable_target() {
    let fixture = Fixture::new(&RUNTIME_BOUND, true).await;
    fixture.seed().await;
    let capabilities = fixture
        .state
        .queries()
        .get_session("session")
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(capabilities.manual_compaction);
    assert!(capabilities.compaction_events);
    assert!(capabilities.compaction_history);
    assert_eq!(
        capabilities.context_usage,
        ContextUsageCapability::Unsupported
    );
    fixture.channel.closed.store(true, Ordering::SeqCst);
    let capabilities = fixture
        .state
        .queries()
        .get_session("session")
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(!capabilities.manual_compaction);
    assert!(capabilities.compaction_events);
    assert!(capabilities.compaction_history);
    sqlx::query("DELETE FROM session_runtimes WHERE session_id='session'")
        .execute(&fixture.state.db())
        .await
        .unwrap();
    let capabilities = fixture
        .state
        .queries()
        .get_session("session")
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(!capabilities.manual_compaction);
    assert!(capabilities.compaction_events);
    assert!(capabilities.compaction_history);
}

#[tokio::test]
async fn a_lost_compaction_receipt_is_replayed_as_unknown_without_a_second_native_send() {
    let fixture = Fixture::new(&SPEC, true).await;
    fixture.seed().await;
    *fixture.channel.error.lock().unwrap() = Some(Error::ControlUnknown("receipt lost".into()));
    let first = fixture
        .request("POST", COMPACT, Some("lost-receipt"), true)
        .await;
    assert_eq!(first.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(first.1["error"]["code"], "control_unknown");
    // The channel can acknowledge new requests, but a replay must retain uncertainty.
    let replay = fixture
        .request("POST", COMPACT, Some("lost-receipt"), true)
        .await;
    assert_eq!(replay, first);
    assert_eq!(fixture.channel.operations.lock().unwrap().len(), 1);
    assert_eq!(
        fixture
            .state
            .queries()
            .list_session_events("session")
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn dynamically_advertised_compaction_can_execute_against_a_default_false_static_spec() {
    use pontia_application::{
        AppState,
        client_contract::{AgentInput, InProcessClient, test_registration},
    };
    struct AdvertisedClient;
    impl InProcessClient for AdvertisedClient {
        fn capabilities(&self) -> AgentClientCapabilities {
            AgentClientCapabilities {
                manual_compaction: true,
                ..AgentClientCapabilities::default()
            }
        }
        fn submit(&self, _: AgentInput) -> pontia_core::Result<()> {
            Ok(())
        }
        fn ready(&self, _: &str, _: &str) -> pontia_core::domain::ReportedEvent {
            unreachable!("this test attaches an existing control target")
        }
    }
    let fixture = Fixture::new(&UNSUPPORTED, true).await;
    fixture.seed().await;
    let mut registration = test_registration();
    registration.spec = &UNSUPPORTED;
    registration.in_process = Some(std::sync::Arc::new(AdvertisedClient));
    let mut clients = fixture.state.clients();
    clients.register(registration);
    let state = AppState::builder(fixture.state.db(), fixture.root.path().into())
        .clients(clients)
        .external_api_token(Some("test-token".into()))
        .build();
    state
        .client_control()
        .attach(
            SPEC.client_type,
            "session",
            "runtime",
            "native",
            fixture.channel.clone(),
        )
        .await
        .unwrap();
    assert!(
        state
            .queries()
            .get_session("session")
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .manual_compaction
    );
    let (status, body) = request(state, "POST", COMPACT, None, true).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(
        fixture.channel.operations.lock().unwrap().as_slice(),
        [body["data"]["operation_id"].as_str().unwrap()]
    );
}
