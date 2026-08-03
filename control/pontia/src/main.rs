use pontia_application as application;
use pontia_config::AppConfig;
use pontia_core::error::Result;
use pontia_http as http;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use pontia_runtime::ClaudeApprovalIntegration;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let bound_addr = listener.local_addr()?;
    match pontia_runtime::configure_claude_user_approval_integration(
        bound_addr,
        config.external_api_token.as_deref(),
    )? {
        ClaudeApprovalIntegration::Configured { settings_path } => {
            info!(
                path = %settings_path.display(),
                "configured Claude approval integration"
            );
        }
        ClaudeApprovalIntegration::SkippedMissingApiToken => {
            warn!(
                "Claude approval integration is disabled because external_api_token is not configured"
            );
        }
    }
    let app_state = application::initialize(&config).await?;
    let runtime_observer = application::RuntimeObservationService::new(app_state.db())
        .with_agent_events(app_state.agent_events());
    tokio::spawn(runtime_observer.run(app_state.shutdown().subscribe()));
    let dashboard = http::dashboard::resolve_dashboard(&config.dashboard).await;
    let state = http::HttpState::new(app_state, dashboard);

    info!(addr = %bound_addr, "starting pontia control plane");
    info!(url = %dashboard_url(bound_addr), "dashboard available");

    let shutdown = state.app().shutdown();
    http::serve_with_shutdown_timeout(
        listener,
        http::router(state),
        async move {
            shutdown_signal().await;
            shutdown.notify();
        },
        Duration::from_secs(5),
    )
    .await?;

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
