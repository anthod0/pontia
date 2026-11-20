use super::{
    common::test_app::TestApp,
    support::http::{TOKEN, get_json, json_response},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn fixture() -> TestApp {
    let app = TestApp::new().await;
    sqlx::query(
        "INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id)
        VALUES ('a','A','missing','test','model','s'), ('b','B','missing','test','model','t')",
    )
    .execute(&app.db)
    .await
    .unwrap();
    app
}

async fn message(app: &TestApp, bot: &str, id: &str, time: &str, status: &str) {
    sqlx::query("INSERT INTO bot_messages(bot_id,id,kind,created_at,status,content) VALUES (?,?,'user_input',?,?,?)")
        .bind(bot).bind(id).bind(time).bind(status).bind(id).execute(&app.db).await.unwrap();
}

async fn event(app: &TestApp, session: &str, id: &str, time: &str, kind: &str) {
    sqlx::query(
        "INSERT INTO events(session_id,event_id,source,client_type,event_type,occurred_at)
        VALUES (?,?,'agent_adapter','generic',?,?)",
    )
    .bind(session)
    .bind(id)
    .bind(kind)
    .bind(time)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn get(app: &TestApp, bot: &str, query: &[(&str, &str)]) -> (StatusCode, Value) {
    let base = format!("/api/v1/bots/{bot}/messages");
    let mut url = reqwest::Url::parse(&format!("http://localhost{base}")).unwrap();
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    get_json(app.state.clone(), &url.as_str()["http://localhost".len()..]).await
}

async fn page(app: &TestApp, query: &[(&str, &str)]) -> Value {
    let (status, body) = get(app, "a", query).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

fn ids(page: &Value) -> Vec<&str> {
    page["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn pages_and_anchors_use_the_same_millisecond_and_binary_id_order() {
    let app = fixture().await;
    // These four rows tie in UTC milliseconds. Fractional precision and time zone differ.
    event(
        &app,
        "s",
        "D",
        "2026-11-20T04:00:00.999999999+04:00",
        "session.error",
    )
    .await;
    event(
        &app,
        "s",
        "B",
        "2026-11-19T22:30:00.9991-01:30",
        "session.exited",
    )
    .await;
    message(&app, "a", "C", "2026-11-20T00:00:00.999Z", "normal").await;
    message(&app, "a", "a", "2026-11-20T00:00:00.999001Z", "normal").await;
    event(
        &app,
        "s",
        "early",
        "2026-11-20T08:00:00.99+08:00",
        "session.error",
    )
    .await;
    message(&app, "a", "last", "2026-11-20T00:00:01Z", "normal").await;
    event(
        &app,
        "s",
        "hidden",
        "2026-12-01T00:00:00Z",
        "turn.completed",
    )
    .await;
    message(&app, "b", "other", "2026-12-01T00:00:00Z", "normal").await;
    event(
        &app,
        "t",
        "foreign-event",
        "2026-12-01T00:00:00Z",
        "session.error",
    )
    .await;

    let expected = ["early", "B", "C", "D", "a", "last"];
    let latest = page(&app, &[]).await;
    assert_eq!(ids(&latest), expected);
    assert_eq!(latest["notice_session_id"], "s");
    assert!(latest["next_before"].is_null());
    assert!(latest["generation_notice"].is_null());
    let mut paged = page(&app, &[("limit", "1")]).await;
    for (index, id) in expected.iter().rev().enumerate() {
        assert_eq!(ids(&paged), [*id]);
        if index + 1 < expected.len() {
            paged = page(
                &app,
                &[
                    ("before", paged["next_before"].as_str().unwrap()),
                    ("limit", "1"),
                ],
            )
            .await;
        }
    }
    assert!(paged["next_before"].is_null());
    let middle = page(&app, &[("around", "D"), ("limit", "3")]).await;
    assert_eq!(ids(&middle), ["C", "D", "a"]);
    let cursor = middle["next_before"].as_str().unwrap();
    assert_eq!(
        ids(&page(&app, &[("before", cursor)]).await),
        ["early", "B"]
    );
    assert_eq!(
        ids(&page(&app, &[("around", "early"), ("limit", "3")]).await),
        ["early", "B", "C"]
    );
    assert_eq!(
        ids(&page(&app, &[("around", "last"), ("limit", "3")]).await),
        ["D", "a", "last"]
    );
    for invisible in ["hidden", "foreign-event", "unknown", "other"] {
        assert_eq!(
            get(&app, "a", &[("around", invisible)]).await.0,
            StatusCode::NOT_FOUND
        );
    }
    for query in [
        vec![("before", "bad")],
        vec![("before", cursor), ("around", "D")],
        vec![("limit", "0")],
        vec![("limit", "101")],
        vec![("limit", "lots")],
    ] {
        assert_eq!(get(&app, "a", &query).await.0, StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        get(&app, "b", &[("before", cursor)]).await.0,
        StatusCode::BAD_REQUEST
    );
    sqlx::query("UPDATE bots SET current_session_id=NULL WHERE bot_id='a'")
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = get(&app, "a", &[("before", cursor)]).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "bot_timeline_reset");
    assert_eq!(
        get(&app, "a", &[("around", "D")]).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        ids(&page(&app, &[("around", "C"), ("limit", "1")]).await),
        ["C"]
    );
}

#[tokio::test]
async fn processing_considers_unanswered_input_outside_the_default_fifty_rows() {
    let app = fixture().await;
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('s','generic','busy')")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','s','running')")
        .execute(&app.db)
        .await
        .unwrap();
    message(&app, "a", "old", "2026-11-20T00:00:00.000Z", "normal").await;
    for n in 0..60 {
        message(
            &app,
            "a",
            &format!("recent-{n:02}"),
            "2026-11-20T01:00:00.000Z",
            "failed",
        )
        .await;
    }
    let tail = page(&app, &[]).await;
    assert_eq!(ids(&tail).len(), 50);
    assert!(
        tail["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["status"] == "failed")
    );
    assert_eq!(tail["processing"], true);
    let legacy = get_json(app.state.clone(), "/api/v1/bots/a").await.1;
    assert_eq!(legacy["data"]["messages"].as_array().unwrap().len(), 61);
    assert_eq!(legacy["data"]["processing"], true);
    sqlx::query("UPDATE turns SET state='completed' WHERE turn_id='turn'")
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(page(&app, &[]).await["processing"], false);
}

async fn open_stream(router: &Router, path: &str) -> Body {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body()
}

async fn next_update(stream: &mut Body) -> Value {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), stream.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let data = std::str::from_utf8(&frame)
        .unwrap()
        .trim()
        .strip_prefix("data:")
        .unwrap()
        .trim();
    serde_json::from_str(data).unwrap()
}

#[tokio::test]
async fn another_clients_retry_updates_the_original_row_off_the_tail_and_reset_recovers_it() {
    let app = fixture().await;
    let router = pontia_http::router(app.state.clone());
    for path in [
        "/api/v1/bots/a/messages",
        "/api/v1/bots/a/stream?timeline=true",
    ] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    message(&app, "a", "retry-me", "2026-01-01T00:00:00Z", "failed").await;
    let mut live = open_stream(&router, "/api/v1/bots/a/stream?timeline=true").await;
    assert_eq!(next_update(&mut live).await, json!({"type":"reset"}));
    let mut legacy = open_stream(&router, "/api/v1/bots/a/stream").await;
    assert_eq!(next_update(&mut legacy).await, "changed");
    for n in 0..55 {
        message(
            &app,
            "a",
            &format!("later-{n}"),
            "2026-02-01T00:00:00Z",
            "normal",
        )
        .await;
    }
    assert!(!ids(&page(&app, &[]).await).contains(&"retry-me"));
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bots/a/messages/retry-me/retry")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = json_response(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The fixture's unavailable client fails delivery after the retry marks the row normal.
    assert_eq!(body["data"]["message"]["status"], "failed");
    let started = next_update(&mut live).await;
    assert_eq!(started["type"], "message");
    assert_eq!(started["message"]["id"], "retry-me");
    assert_eq!(started["message"]["status"], "normal");
    let failed = loop {
        let update = next_update(&mut live).await;
        if update["type"] == "message" {
            break update;
        }
    };
    assert_eq!(failed["message"], body["data"]["message"]);
    assert_eq!(next_update(&mut legacy).await, "changed");
    let mut reconnected = open_stream(&router, "/api/v1/bots/a/stream?timeline=true").await;
    assert_eq!(next_update(&mut reconnected).await, json!({"type":"reset"}));
    let anchored = page(&app, &[("around", "retry-me"), ("limit", "1")]).await;
    assert_eq!(anchored["entries"][0], body["data"]["message"]);
}

#[tokio::test]
async fn streaming_resets_for_current_session_notices_and_missed_updates() {
    use pontia_core::domain::{EventSource, EventType, ReportedEvent};

    let app = fixture().await;
    let state = pontia_http::HttpState::from(app.state.clone());
    let router = pontia_http::router(state.clone());
    let events = app.state.control().event_ingest_service();
    for session in ["s", "t"] {
        events
            .ingest_reported_event(ReportedEvent::new(
                format!("created-{session}"),
                session.into(),
                None,
                EventSource::AgentAdapter,
                "generic".into(),
                EventType::SessionCreated,
                json!({}),
            ))
            .await
            .unwrap();
    }
    let mut live = open_stream(&router, "/api/v1/bots/a/stream?timeline=true").await;
    assert_eq!(next_update(&mut live).await, json!({"type":"reset"}));
    for session in ["t", "s"] {
        events
            .ingest_reported_event(ReportedEvent::new(
                format!("exit-{session}"),
                session.into(),
                None,
                EventSource::AgentAdapter,
                "generic".into(),
                EventType::SessionExited,
                json!({}),
            ))
            .await
            .unwrap();
    }
    assert_eq!(next_update(&mut live).await, json!({"type":"reset"}));
    let notices = page(&app, &[]).await;
    assert_eq!(ids(&notices), ["exit-s"]);

    // Stop polling this subscription while another client repeatedly retries the same row.
    sqlx::query("UPDATE bots SET current_session_id=NULL WHERE bot_id='a'")
        .execute(&app.db)
        .await
        .unwrap();
    message(&app, "a", "older", "2026-01-01T00:00:00Z", "failed").await;
    for _ in 0..40 {
        state.bots().retry("a", "older").await.unwrap();
    }
    assert_eq!(next_update(&mut live).await, json!({"type":"reset"}));
}
