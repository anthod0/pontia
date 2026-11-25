use super::fixture::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_core::Error;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn seed_bot(fixture: &Fixture) {
    fixture.seed().await;
    sqlx::query(
        r#"INSERT INTO sessions(session_id,client_type,state,application_id,application_ref,credential_env, credential_sha256)
        VALUES ('bot-session','compaction-test','idle','bot','bot:1','PONTIA_BOT_TOOL_TOKEN', x'df27f9beb68b7766af3ab2cd7eeefe0c759ca4d085db8b2235811ad36f27cd1c');
        INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id)
        VALUES ('bot','Bot','compaction-test','default','test','bot-session');
        INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES ('helper:bot:1','bot','session');"#,
    )
    .execute(&fixture.state.db())
    .await
    .unwrap();
}

async fn compact(fixture: &Fixture, token: &str) -> (StatusCode, Value) {
    let response = pontia_http::router(fixture.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bot/helper")
                .header("Authorization", "Bearer test-token")
                .header("Content-Type", "application/json")
                .header("x-pontia-bot-tool-token", token)
                .body(Body::from(
                    json!({
                        "session_id":"bot-session",
                        "command":{"action":"compact", "input":{"session_id":"session"}}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn helper_compaction_authenticates_then_returns_the_native_operation_receipt() {
    let fixture = Fixture::new(&SPEC, true).await;
    seed_bot(&fixture).await;
    assert_eq!(
        compact(&fixture, "wrong-token").await.0,
        StatusCode::FORBIDDEN
    );
    assert!(fixture.channel.operations.lock().unwrap().is_empty());
    for state in ["idle", "interrupted"] {
        fixture.set_state(state).await;
        let (status, body) = compact(&fixture, "bot-token").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["accepted"], true);
        let operation = body["data"]["operation_id"].as_str().unwrap();
        assert!(!operation.is_empty());
        assert_eq!(
            fixture.channel.operations.lock().unwrap().last().unwrap(),
            operation
        );
        assert_eq!(
            fixture
                .state
                .control()
                .queries()
                .get_session("session")
                .await
                .unwrap()
                .unwrap()
                .state,
            state
        );
    }
    assert_eq!(fixture.channel.operations.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn helper_compaction_preserves_capability_state_and_unknown_errors() {
    let fixture = Fixture::new(&UNSUPPORTED, true).await;
    seed_bot(&fixture).await;
    let (status, body) = compact(&fixture, "bot-token").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "capability_unavailable");
    assert!(fixture.channel.operations.lock().unwrap().is_empty());

    let fixture = Fixture::new(&SPEC, true).await;
    seed_bot(&fixture).await;
    for state in ["busy", "exited"] {
        fixture.set_state(state).await;
        let (status, body) = compact(&fixture, "bot-token").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "state_conflict");
    }
    assert!(fixture.channel.operations.lock().unwrap().is_empty());
    let fixture = Fixture::new(&SPEC, true).await;
    seed_bot(&fixture).await;
    *fixture.channel.error.lock().unwrap() = Some(Error::ControlUnknown("receipt lost".into()));
    let (status, body) = compact(&fixture, "bot-token").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "control_unknown");
    assert_eq!(fixture.channel.operations.lock().unwrap().len(), 1);
}
