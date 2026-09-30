mod support;

use axum::http::{StatusCode, header};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use support::TestEdge;
use uuid::Uuid;

fn cookie_value(response: &reqwest::Response) -> String {
    let value = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(value.starts_with("__Host-pontia_edge_access=pba_v1_"));
    assert!(value.contains("; Path=/"));
    assert!(value.contains("; Expires="));
    assert!(value.contains("; Max-Age="));
    assert!(value.contains("; Secure"));
    assert!(value.contains("; HttpOnly"));
    assert!(value.contains("; SameSite=Strict"));
    assert!(!value.to_ascii_lowercase().contains("domain="));
    value.split(';').next().unwrap().to_owned()
}

async fn bootstrap(server: &TestEdge, ticket: &str, cookie: Option<&str>) -> reqwest::Response {
    let mut request = server
        .http
        .post(format!("{}/dashboard/bootstrap", server.edge_origin))
        .header(header::ORIGIN, "https://pontia.dev")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("ticket={ticket}"));
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.send().await.unwrap()
}

#[tokio::test]
async fn bootstrap_sets_a_host_cookie_redirects_and_authorizes_only_redeemed_devices() {
    let server = TestEdge::start().await;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let unauthorized = Uuid::new_v4();
    let first_ticket = server.issue_dashboard_ticket(first, "first-device");

    let response = bootstrap(&server, &first_ticket, None).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "https://app.pontia.dev/first-device"
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(
        response.headers().get("referrer-policy").unwrap(),
        "no-referrer"
    );
    let cookie = cookie_value(&response);
    let body = response.text().await.unwrap();
    assert!(!body.contains(&first_ticket));

    let authorized = server
        .http
        .get(format!(
            "{}/devices/{first}/api/v1/sessions",
            server.edge_origin
        ))
        .header(header::ORIGIN, "https://app.pontia.dev")
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::NOT_IMPLEMENTED);
    assert_eq!(
        authorized
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://app.pontia.dev"
    );
    assert_eq!(
        authorized
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap(),
        "true"
    );
    assert_eq!(authorized.headers().get(header::VARY).unwrap(), "Origin");

    let denied = server
        .http
        .get(format!(
            "{}/devices/{unauthorized}/api/v1/sessions",
            server.edge_origin
        ))
        .header(header::ORIGIN, "https://app.pontia.dev")
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let second_ticket = server.issue_dashboard_ticket(second, "second-device");
    let second_response = bootstrap(&server, &second_ticket, Some(&cookie)).await;
    assert_eq!(second_response.status(), StatusCode::SEE_OTHER);
    assert_eq!(cookie_value(&second_response), cookie);

    for device in [first, second] {
        let response = server
            .http
            .get(format!(
                "{}/devices/{device}/api/v1/sessions",
                server.edge_origin
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}

#[tokio::test]
async fn bootstrap_rejects_wrong_origins_fields_and_untrusted_website_responses_without_leaking() {
    let server = TestEdge::start().await;
    let device = Uuid::new_v4();
    let ticket = server.issue_dashboard_ticket(device, "valid-device");
    let wrong_origin = server
        .http
        .post(format!("{}/dashboard/bootstrap", server.edge_origin))
        .header(header::ORIGIN, "https://app.pontia.dev")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("ticket={ticket}"))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_origin.status(), StatusCode::BAD_REQUEST);
    assert!(wrong_origin.headers().get(header::SET_COOKIE).is_none());
    assert!(!wrong_origin.text().await.unwrap().contains(&ticket));

    let query_ticket = server.issue_dashboard_ticket(device, "valid-device");
    let query = server
        .http
        .post(format!(
            "{}/dashboard/bootstrap?ticket={query_ticket}",
            server.edge_origin
        ))
        .header(header::ORIGIN, "https://pontia.dev")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("")
        .send()
        .await
        .unwrap();
    assert_eq!(query.status(), StatusCode::BAD_REQUEST);
    assert!(query.headers().get(header::SET_COOKIE).is_none());

    let extra_ticket = server.issue_dashboard_ticket(device, "valid-device");
    let extra = server
        .http
        .post(format!("{}/dashboard/bootstrap", server.edge_origin))
        .header(header::ORIGIN, "https://pontia.dev")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("ticket={extra_ticket}&device_id={device}"))
        .send()
        .await
        .unwrap();
    assert_eq!(extra.status(), StatusCode::BAD_REQUEST);
    assert!(extra.headers().get(header::SET_COOKIE).is_none());

    let malformed_ticket = format!("pet_v1_{}", "A".repeat(43));
    server.set_dashboard_response(
        malformed_ticket.clone(),
        json!({
            "device_id": device,
            "device_handle": "valid-device",
            "expires_at": "2099-01-01T00:00:00.000Z",
            "user_id": "must-not-be-accepted"
        }),
    );
    let malformed = bootstrap(&server, &malformed_ticket, None).await;
    assert_eq!(malformed.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(malformed.headers().get(header::SET_COOKIE).is_none());
    assert!(!malformed.text().await.unwrap().contains(&malformed_ticket));

    for (suffix, response) in [
        (
            'Q',
            json!({
                "device_id": device.to_string().to_uppercase(),
                "device_handle": "valid-device",
                "expires_at": "2099-01-01T00:00:00.000Z"
            }),
        ),
        (
            'g',
            json!({
                "device_id": device,
                "device_handle": "settings",
                "expires_at": "2099-01-01T00:00:00.000Z"
            }),
        ),
        (
            'w',
            json!({
                "device_id": device,
                "device_handle": "valid-device",
                "expires_at": "2099-01-01T00:00:00Z"
            }),
        ),
    ] {
        let ticket = format!("pet_v1_{}{suffix}", "A".repeat(42));
        server.set_dashboard_response(ticket.clone(), response);
        let invalid = bootstrap(&server, &ticket, None).await;
        assert_eq!(invalid.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(invalid.headers().get(header::SET_COOKIE).is_none());
    }

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new().filename(server.root.path().join("edge.sqlite3")),
    )
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_browser_access BEFORE INSERT ON browser_device_access \
         BEGIN SELECT RAISE(FAIL, 'injected persistence failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let persistence_ticket = server.issue_dashboard_ticket(device, "valid-device");
    let persistence_failure = bootstrap(&server, &persistence_ticket, None).await;
    assert_eq!(
        persistence_failure.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(
        persistence_failure
            .headers()
            .get(header::SET_COOKIE)
            .is_none()
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_device_access")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn bootstrap_fails_closed_when_the_website_is_unavailable() {
    let server = TestEdge::start().await;
    let ticket = server.issue_dashboard_ticket(Uuid::new_v4(), "offline-device");
    server.stop_website();

    let response = bootstrap(&server, &ticket, None).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    assert!(!response.text().await.unwrap().contains(&ticket));
}

#[tokio::test]
async fn device_boundary_enforces_cors_and_csrf_before_proxying() {
    let server = TestEdge::start().await;
    let device = Uuid::new_v4();
    let ticket = server.issue_dashboard_ticket(device, "cors-device");
    let cookie = cookie_value(&bootstrap(&server, &ticket, None).await);
    let url = format!("{}/devices/{device}/api/v1/sessions", server.edge_origin);

    let foreign = server
        .http
        .get(&url)
        .header(header::ORIGIN, "https://evil.example")
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    assert!(
        foreign
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );

    let missing_origin = server
        .http
        .post(&url)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(missing_origin.status(), StatusCode::FORBIDDEN);

    let preflight = server
        .http
        .request(reqwest::Method::OPTIONS, &url)
        .header(header::ORIGIN, "https://app.pontia.dev")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        preflight
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "https://app.pontia.dev"
    );
}
