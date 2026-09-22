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
        pontia_tunnel::RemoteClient::new(&remote.edge_url, identity, remote.ca_certificate.as_deref())
    }).transpose().map_err(|error| pontia_core::error::Error::InvalidConfig {
        key: "remote",
        message: error.to_string(),
    })?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let bound_addr = listener.local_addr()?;
    let app_state = application::initialize(&config).await?;
    let remote_task =
        remote.map(|remote| tokio::spawn(remote.run(app_state.shutdown().subscribe())));
    pontia_runtime::set_runtime_bind_addr(bound_addr);
    tokio::spawn(
        application::codex::CodexObserver::new(
            app_state.db(),
            app_state.pontia_home().to_path_buf(),
        )
        .run(app_state.shutdown().subscribe()),
    );
    let pi_control = app_state.pi_control();
    let pi_shutdown = app_state.shutdown().subscribe();
    let pi_control_task = tokio::spawn(async move { pi_control.run(pi_shutdown).await });
    let runtime_observer = application::RuntimeObservationService::new(app_state.db())
        .with_agent_events(app_state.agent_events())
        .with_live_output(app_state.live_output());
    tokio::spawn(runtime_observer.run(app_state.shutdown().subscribe()));
    let workflow_coordinator = pontia_workflow::WorkflowCoordinator::new(
        app_state.db(),
        application::SessionCommandService::new(
            app_state.db(),
            app_state.pontia_home().to_path_buf(),
        ),
        app_state.agent_events(),
        app_state.pontia_home().to_path_buf(),
    );
    tokio::spawn(workflow_coordinator.run(app_state.shutdown().subscribe()));
    let dashboard =
        http::dashboard::resolve_dashboard(&config.dashboard, &config.pontia_home).await;
    let state = http::HttpState::new(app_state, dashboard);

    info!(addr = %bound_addr, "starting pontia control plane");
    info!(url = %dashboard_url(bound_addr), "dashboard available");

    let shutdown = state.app().shutdown();
    let cleanup_shutdown = shutdown.clone();
    let codex_root = state.app().pontia_home().to_path_buf();
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
    let _ = pi_control_task.await;
    if let Some(task) = remote_task {
        let _ = task.await;
    }

    pontia_runtime::codex::CodexRuntime::shutdown(&codex_root).await;

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
