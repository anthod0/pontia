use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use axum_server::{Handle, tls_rustls::RustlsConfig};
use futures_util::{SinkExt, StreamExt};
use pontia_edge::{ConnectionLimits, Edge, TicketRedeemer};
use pontia_tunnel::protocol::Message;
use rustls::{ClientConfig, RootCertStore, ServerConfig, pki_types::PrivatePkcs8KeyDer};
use serde::Deserialize;
use tokio::{net::TcpStream, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
    tungstenite::{Message as WsMessage, client::IntoClientRequest},
};
use uuid::Uuid;

pub type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub const CLI_CREDENTIAL: &str = "ptr_v1_session_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
pub const EDGE_CREDENTIAL: &str =
    "pec_v1_01a0e686-24d4-75e8-866d-5710ffe3b2b5_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn new_ticket() -> String {
    format!("pet_v1_{}{}", Uuid::new_v4().simple(), "A".repeat(11))
}

#[derive(Clone)]
struct WebsiteState {
    tickets: Arc<Mutex<HashMap<String, Uuid>>>,
    issued: Arc<AtomicUsize>,
    delay_redemption: Arc<AtomicBool>,
    tunnel_url: String,
}

#[derive(Deserialize)]
struct RedeemRequest {
    ticket: String,
}

pub struct TestEdge {
    pub root: tempfile::TempDir,
    pub edge: Edge,
    pub tunnel_url: String,
    pub website_origin: String,
    pub connector: Connector,
    pub http: reqwest::Client,
    tickets: Arc<Mutex<HashMap<String, Uuid>>>,
    issued: Arc<AtomicUsize>,
    delay_redemption: Arc<AtomicBool>,
    edge_handle: Handle<std::net::SocketAddr>,
    edge_task: JoinHandle<std::io::Result<()>>,
    website_handle: Handle<std::net::SocketAddr>,
    website_task: JoinHandle<std::io::Result<()>>,
}

impl TestEdge {
    pub async fn start() -> Self {
        let root = tempfile::tempdir().unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server_tls = || {
            ServerConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![cert.der().clone()],
                    PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
                )
                .unwrap()
        };
        let mut roots = RootCertStore::empty();
        roots.add(cert.der().clone()).unwrap();
        let client_tls = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = Connector::Rustls(Arc::new(client_tls));
        let http = reqwest::Client::builder()
            .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();

        let edge_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        edge_listener.set_nonblocking(true).unwrap();
        let url = format!(
            "wss://127.0.0.1:{}/tunnel",
            edge_listener.local_addr().unwrap().port()
        );
        let website_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        website_listener.set_nonblocking(true).unwrap();
        let website_origin = format!(
            "https://127.0.0.1:{}",
            website_listener.local_addr().unwrap().port()
        );
        let tickets = Arc::new(Mutex::new(HashMap::new()));
        let issued = Arc::new(AtomicUsize::new(0));
        let delay_redemption = Arc::new(AtomicBool::new(false));
        let website_state = WebsiteState {
            tickets: tickets.clone(),
            issued: issued.clone(),
            delay_redemption: delay_redemption.clone(),
            tunnel_url: url.clone(),
        };
        let website = Router::new()
            .route(
                "/api/remote/devices/{device_id}/tunnel-tickets",
                post(issue),
            )
            .route("/api/edge/tunnel-tickets/redeem", post(redeem))
            .with_state(website_state);
        let website_handle = Handle::new();
        let website_task = tokio::spawn(
            axum_server::from_tcp_rustls(
                website_listener,
                RustlsConfig::from_config(Arc::new(server_tls())),
            )
            .unwrap()
            .handle(website_handle.clone())
            .serve(website.into_make_service()),
        );

        let redeemer =
            TicketRedeemer::with_client(&website_origin, EDGE_CREDENTIAL.to_string(), http.clone())
                .unwrap();
        let edge = Edge::new(
            redeemer,
            ConnectionLimits {
                max_pending: 2,
                ticket_redeem_timeout: Duration::from_millis(500),
                heartbeat_interval: Duration::from_millis(100),
                pong_timeout: Duration::from_millis(200),
            },
        );
        let edge_handle = Handle::new();
        let edge_task = tokio::spawn(
            axum_server::from_tcp_rustls(
                edge_listener,
                RustlsConfig::from_config(Arc::new(server_tls())),
            )
            .unwrap()
            .handle(edge_handle.clone())
            .serve(edge.router().into_make_service()),
        );
        Self {
            root,
            edge,
            tunnel_url: url,
            website_origin,
            connector,
            http,
            tickets,
            issued,
            delay_redemption,
            edge_handle,
            edge_task,
            website_handle,
            website_task,
        }
    }

    pub fn issued_count(&self) -> usize {
        self.issued.load(Ordering::SeqCst)
    }

    pub fn stop_website(&self) {
        self.website_handle.shutdown();
    }

    pub fn delay_redemption(&self) {
        self.delay_redemption.store(true, Ordering::SeqCst);
    }

    pub fn issue_ticket(&self, device_id: Uuid) -> String {
        let ticket = new_ticket();
        self.tickets
            .lock()
            .unwrap()
            .insert(ticket.clone(), device_id);
        ticket
    }

    pub async fn connect(
        &self,
        ticket: Option<&str>,
    ) -> Result<Socket, tokio_tungstenite::tungstenite::Error> {
        let mut request = self.tunnel_url.as_str().into_client_request().unwrap();
        if let Some(ticket) = ticket {
            request.headers_mut().insert(
                header::AUTHORIZATION,
                format!("Bearer {ticket}").parse().unwrap(),
            );
        }
        timeout(
            Duration::from_secs(3),
            connect_async_tls_with_config(request, None, false, Some(self.connector.clone())),
        )
        .await
        .unwrap()
        .map(|connected| connected.0)
    }

    pub async fn authenticate(&self, device_id: Uuid) -> Socket {
        self.connect(Some(&self.issue_ticket(device_id)))
            .await
            .unwrap()
    }
}

impl Drop for TestEdge {
    fn drop(&mut self) {
        self.edge.shutdown();
        self.edge_handle.shutdown();
        self.website_handle.shutdown();
        self.edge_task.abort();
        self.website_task.abort();
    }
}

async fn issue(
    State(state): State<WebsiteState>,
    Path(device_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        != Some(&format!("Bearer {CLI_CREDENTIAL}"))
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let ticket = new_ticket();
    state.issued.fetch_add(1, Ordering::SeqCst);
    state
        .tickets
        .lock()
        .unwrap()
        .insert(ticket.clone(), device_id);
    Ok(Json(serde_json::json!({
        "ticket": ticket,
        "tunnel_url": state.tunnel_url,
        "expires_at": "2099-01-01T00:00:00.000Z"
    })))
}

async fn redeem(
    State(state): State<WebsiteState>,
    headers: HeaderMap,
    Json(request): Json<RedeemRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        != Some(&format!("Bearer {EDGE_CREDENTIAL}"))
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if state.delay_redemption.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let device_id = state
        .tickets
        .lock()
        .unwrap()
        .remove(&request.ticket)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(serde_json::json!({ "device_id": device_id })))
}

pub async fn receive(socket: &mut Socket) -> Message {
    let message = timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let WsMessage::Text(text) = message else {
        panic!("expected text, got {message:?}")
    };
    serde_json::from_str(&text).unwrap()
}

pub async fn send(socket: &mut Socket, message: Message) {
    socket
        .send(WsMessage::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

pub async fn closed(socket: &mut Socket) {
    match timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
    {
        None | Some(Err(_)) | Some(Ok(WsMessage::Close(_))) => {}
        other => panic!("expected closed connection, got {other:?}"),
    }
}

pub async fn wait_for(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition should become true");
}
