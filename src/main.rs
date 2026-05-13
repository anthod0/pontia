use llmparty::{
    application,
    config::{AppConfig, config_path_from_args},
    transport::http,
};
use std::time::Duration;

use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> llmparty::error::Result<()> {
    init_tracing();

    let config_path = config_path_from_args(std::env::args())?;
    let config = AppConfig::from_env_with_config_path(config_path.as_deref())?;
    let state = application::initialize(&config).await?;

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    info!(addr = %config.bind_addr, "starting llmparty control plane");

    http::serve_with_shutdown_timeout(
        listener,
        http::router(state),
        shutdown_signal(),
        Duration::from_secs(5),
    )
    .await?;

    Ok(())
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
