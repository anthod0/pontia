use llmparty::{
    application,
    config::{AppConfig, config_path_from_args},
    transport::http,
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> llmparty::error::Result<()> {
    init_tracing();

    let config_path = config_path_from_args(std::env::args())?;
    let config = AppConfig::from_env_with_config_path(config_path.as_deref())?;
    let state = application::initialize(&config).await?;

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let bound_addr = listener.local_addr()?;
    info!(addr = %bound_addr, "starting llmparty control plane");
    info!(url = %dashboard_url(bound_addr), "dashboard available");

    http::serve_with_shutdown_timeout(
        listener,
        http::router(state),
        shutdown_signal(),
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
                .unwrap_or_else(|_| "llmparty=info,tower_http=info".into()),
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
