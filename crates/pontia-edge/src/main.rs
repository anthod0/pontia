use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::Result;
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use pontia_edge::{ConnectionLimits, DeviceBindings, Edge};

#[derive(Parser)]
#[command(about = "Pontia device connection service")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8443")]
    bind: SocketAddr,
    #[arg(long)]
    database: PathBuf,
    #[arg(long)]
    tls_cert: PathBuf,
    #[arg(long)]
    tls_key: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pontia=info".into()),
        )
        .init();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = RustlsConfig::from_pem_file(args.tls_cert, args.tls_key).await?;
    let edge = Edge::new(
        DeviceBindings::open(&args.database).await?,
        ConnectionLimits::default(),
    );
    let handle = axum_server::Handle::new();
    let signal_edge = edge.clone();
    let signal_handle = handle.clone();
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        signal_edge.shutdown();
        signal_handle.graceful_shutdown(Some(Duration::from_secs(5)));
    });
    tracing::info!(addr = %args.bind, "starting edge");
    let result = axum_server::bind_rustls(args.bind, tls)
        .handle(handle)
        .serve(edge.router().into_make_service())
        .await;
    edge.shutdown();
    signal.abort();
    result?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate => {}
    }
}
