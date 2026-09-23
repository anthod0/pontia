mod initialization;
use pontia_application as application;
use pontia_config::AppConfig;
use pontia_core::error::Result;
use pontia_http as http;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env()?;
    init_tracing();
    let remote = config.remote.as_ref().map(|remote| {
        let identity = pontia_tunnel::DeviceIdentity::load_or_create(
            &config.pontia_home.join("state/device-identity.json"),
        )?;
        info!(device_id = %identity.device_id(), public_key = ?identity.public_key(), "remote device identity");
        pontia_tunnel::RemoteClient::new(&remote.edge_url, identity, remote.access_key.clone(), remote.ca_certificate.as_deref())
    }).transpose().map_err(|error| pontia_core::error::Error::InvalidConfig {
        key: "remote",
        message: error.to_string(),
    })?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let bound_addr = listener.local_addr()?;
    let app_state = initialization::initialize(&config).await?;
    let pi_listener = pontia_client_pi::ipc::PiIpcListener::bind(&config.pontia_home).await?;
    let pi_task =
        tokio::spawn(pi_listener.run(app_state.clone(), app_state.shutdown().subscribe()));
    let remote_task =
        remote.map(|remote| tokio::spawn(remote.run(app_state.shutdown().subscribe())));
    tokio::spawn(
        pontia_client_codex::CodexObserver::new(
            app_state.event_ingest_service(),
            app_state.pontia_home().to_path_buf(),
        )
        .run(app_state.shutdown().subscribe()),
    );
    let inbox = application::InboxCommandService::new(app_state.event_ingest_service());
    let client_control = app_state.client_control();
    let runtime_observer =
        application::RuntimeObservationService::new(app_state.event_ingest_service());
    tokio::spawn(runtime_observer.run(app_state.shutdown().subscribe()));
    let workflow_coordinator = pontia_workflow::WorkflowCoordinator::new(
        app_state.event_ingest_service(),
        application::SessionCommandService::new(
            app_state.event_ingest_service(),
            app_state.pontia_home().to_path_buf(),
        )
        .with_client_control(app_state.client_control()),
        app_state.agent_events(),
        app_state.pontia_home().to_path_buf(),
    );
    let workflow_coordinator = workflow_coordinator.with_client_control(app_state.client_control());
    tokio::spawn(workflow_coordinator.run(app_state.shutdown().subscribe()));
    let dashboard =
        http::dashboard::resolve_dashboard(&config.dashboard, &config.pontia_home).await;
    let state = http::HttpState::new(app_state, dashboard);

    info!(addr = %bound_addr, "starting pontia control plane");
    info!(url = %dashboard_url(bound_addr), "dashboard available");

    let shutdown = state.app().shutdown();
    let cleanup_shutdown = shutdown.clone();
    let codex_root = state.app().pontia_home().to_path_buf();
    inbox.resume_pending().await?;
    let server_result = http::serve_with_shutdown_timeout(
        listener,
        http::router(state),
        async move {
            shutdown_signal().await;
            shutdown.notify();
        },
        Duration::from_secs(5),
    )
    .await;

    cleanup_shutdown.notify();
    client_control.close().await;
    pi_task
        .await
        .map_err(|error| pontia_core::Error::Domain(error.to_string()))??;
    inbox.stop_scheduling().await;
    if let Some(task) = remote_task {
        let _ = task.await;
    }

    pontia_client_codex::runtime::CodexRuntime::shutdown(&codex_root).await;

    server_result?;

    Ok(())
}

fn dashboard_url(addr: SocketAddr) -> String {
    let host = if addr.ip().is_unspecified() {
        match addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    } else {
        addr.ip()
    };

    format!("http://{}/dashboard", SocketAddr::new(host, addr.port()))
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pontia=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::dashboard_url;
    use std::net::SocketAddr;

    #[test]
    fn dashboard_url_uses_loopback_for_unspecified_bind_address() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().expect("valid socket addr");

        assert_eq!(dashboard_url(addr), "http://127.0.0.1:8080/dashboard");
    }

    #[test]
    fn dashboard_url_uses_configured_bind_address() {
        let addr: SocketAddr = "127.0.0.1:9090".parse().expect("valid socket addr");

        assert_eq!(dashboard_url(addr), "http://127.0.0.1:9090/dashboard");
    }
}
