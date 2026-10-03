use std::sync::{Arc, Mutex};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, Uri},
    response::IntoResponse,
};
use serde_json::json;

use super::*;

struct LocalHttp(reqwest::Client);

impl instant_acme::HttpClient for LocalHttp {
    fn request(
        &self,
        request: axum::http::Request<instant_acme::BodyWrapper<axum::body::Bytes>>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<instant_acme::BytesResponse, instant_acme::Error>,
                > + Send,
        >,
    > {
        use http_body_util::BodyExt;
        let client = self.0.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let response = client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body.collect().await.unwrap().to_bytes())
                .send()
                .await
                .map_err(|error| instant_acme::Error::Other(Box::new(error)))?;
            let mut result = axum::http::Response::builder().status(response.status());
            *result.headers_mut().unwrap() = response.headers().clone();
            let body = response
                .bytes()
                .await
                .map_err(|error| instant_acme::Error::Other(Box::new(error)))?;
            Ok(result.body(axum::body::Body::from(body)).unwrap().into())
        })
    }
}

#[derive(Default)]
struct CaState {
    origin: String,
    accounts: usize,
    orders: usize,
    ready: bool,
    finalized: bool,
    txt: Option<String>,
    challenges: Vec<String>,
    stall_http: bool,
}

async fn ca(State(state): State<Arc<Mutex<CaState>>>, uri: Uri) -> impl IntoResponse {
    let stall_http = state.lock().unwrap().stall_http;
    if uri.path() == "/http" && stall_http {
        std::future::pending::<()>().await;
    }
    let mut state = state.lock().unwrap();
    let base = state.origin.clone();
    let mut headers = HeaderMap::new();
    headers.insert("replay-nonce", "fresh-nonce".parse().unwrap());
    let body = match uri.path() {
        "/directory" => {
            json!({ "newNonce": format!("{base}/nonce"), "newAccount": format!("{base}/account"), "newOrder": format!("{base}/new-order") })
        }
        "/nonce" => json!({}),
        "/account" => {
            state.accounts += 1;
            headers.insert("location", format!("{base}/account/1").parse().unwrap());
            json!({ "status": "valid" })
        }
        "/auth" => json!({
            "identifier": { "type": "dns", "value": "brave-atlas.edge.pontia.dev" }, "status": "pending",
            "challenges": [
                { "type": "dns-01", "url": format!("{base}/dns"), "token": format!("token-{}", state.orders), "status": "pending" },
                { "type": "http-01", "url": format!("{base}/http"), "token": format!("token-{}", state.orders), "status": "pending" },
            ]
        }),
        "/dns" | "/http" => {
            if uri.path() == "/dns" {
                assert!(
                    state.txt.is_some(),
                    "TXT must propagate before notifying CA"
                );
            }
            state.challenges.push(uri.path().to_owned());
            state.ready = true;
            json!({ "type": if uri.path() == "/dns" { "dns-01" } else { "http-01" }, "url": format!("{base}{}", uri.path()), "token": format!("token-{}", state.orders), "status": "valid" })
        }
        "/certificate" => return (headers, "issued-certificate".to_owned()).into_response(),
        "/new-order" | "/order" | "/finalize" => {
            if uri.path() == "/new-order" {
                state.orders += 1;
                state.ready = false;
                state.finalized = false;
                headers.insert("location", format!("{base}/order").parse().unwrap());
            }
            if uri.path() == "/finalize" {
                assert!(state.ready);
                state.finalized = true;
            }
            json!({
                "status": if state.finalized { "valid" } else if state.ready { "ready" } else { "pending" },
                "authorizations": [format!("{base}/auth")], "finalize": format!("{base}/finalize"),
                "certificate": if state.finalized { Some(format!("{base}/certificate")) } else { None }
            })
        }
        path => panic!("unexpected CA request: {path}"),
    };
    (headers, axum::Json(body)).into_response()
}

#[tokio::test]
async fn http_issuance_timeout_cleans_up_shared_challenge_tokens() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(Mutex::new(CaState {
        origin: origin.clone(),
        stall_http: true,
        ..Default::default()
    }));
    let router = Router::new().fallback(ca).with_state(state);
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let account = Account::builder_with_http(Box::new(LocalHttp(reqwest::Client::new())))
        .create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            format!("{origin}/directory"),
            None,
        )
        .await
        .unwrap()
        .0;
    let challenges = ChallengeResponses::default();
    let shared = challenges.clone();
    let issuance =
        tokio::spawn(
            async move { issue_http(account, "brave-atlas.edge.pontia.dev", shared).await },
        );
    tokio::time::timeout(Duration::from_secs(3), async {
        while !challenges.is_active().await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(180)).await;
    let error = issuance.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("timed out after 3 minutes"));
    assert!(!challenges.is_active().await);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn dns_initial_issuance_and_renewal_reuse_local_account_and_keep_private_keys_local() {
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(Mutex::new(CaState {
        origin: origin.clone(),
        ..Default::default()
    }));
    let router = Router::new().fallback(ca).with_state(state.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let issuer = InstantAcmeIssuer {
        account_path: root.path().join("account.json"),
        directory_url: format!("{origin}/directory"),
    };
    let mut values = Vec::new();
    for _ in 0..2 {
        let account = issuer
            .load_account(Account::builder_with_http(Box::new(LocalHttp(
                reqwest::Client::new(),
            ))))
            .await
            .unwrap();
        let order = prepare_dns_order(account, "brave-atlas.edge.pontia.dev")
            .await
            .unwrap();
        let value = order.value.clone().unwrap();
        assert_eq!(value.len(), 43);
        assert!(!state.lock().unwrap().ready);
        state.lock().unwrap().txt = Some(value.clone());
        let certificate = order.validate_and_issue().await.unwrap();
        assert_eq!(certificate.certificate_pem, "issued-certificate");
        assert!(certificate.private_key_pem.contains("PRIVATE KEY"));
        values.push(value);
        state.lock().unwrap().txt = None;
    }
    assert_ne!(values[0], values[1]);
    assert_eq!(state.lock().unwrap().accounts, 1);
    assert_eq!(state.lock().unwrap().challenges, ["/dns", "/dns"]);
    // HTTP-01 remains a separate choice even when the same account supports DNS-01.
    let account = issuer
        .load_account(Account::builder_with_http(Box::new(LocalHttp(
            reqwest::Client::new(),
        ))))
        .await
        .unwrap();
    issue_http(
        account,
        "brave-atlas.edge.pontia.dev",
        ChallengeResponses::default(),
    )
    .await
    .unwrap();
    assert_eq!(state.lock().unwrap().challenges, ["/dns", "/dns", "/http"]);
    task.abort();
    let _ = task.await;
}
