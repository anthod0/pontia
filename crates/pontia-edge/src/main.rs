use std::{io::IsTerminal, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use clap::{Args, Parser, Subcommand};
use pontia_edge::{
    ConnectionLimits, Edge, TicketRedeemer,
    credential::{CREDENTIAL_PATH, ensure_root, initialize_credential, read_credential},
};

#[derive(Parser)]
#[command(
    about = "Pontia device connection service",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Generate and save this edge's service credential.
    Init,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1:8443")]
    bind: SocketAddr,
    #[arg(long)]
    tls_cert: Option<PathBuf>,
    #[arg(long)]
    tls_key: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Init) => init(),
        None => serve(cli.serve).await,
    }
}

fn init() -> Result<()> {
    anyhow::ensure!(
        std::io::stdout().is_terminal(),
        "pontia-edge init requires an interactive terminal"
    );
    ensure_root(rustix::process::geteuid().as_raw())?;
    let credential = initialize_credential(CREDENTIAL_PATH.as_ref())?;
    println!("Edge ID: {}", credential.edge_id);
    println!("Credential: {}", credential.value);
    Ok(())
}

async fn serve(args: ServeArgs) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pontia=info".into()),
        )
        .init();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let website_origin =
        std::env::var("PONTIA_WEBSITE_ORIGIN").context("PONTIA_WEBSITE_ORIGIN is required")?;
    let service_credential = read_credential(CREDENTIAL_PATH.as_ref())?;
    let tls_cert = args.tls_cert.context("--tls-cert is required")?;
    let tls_key = args.tls_key.context("--tls-key is required")?;
    let tls = RustlsConfig::from_pem_file(tls_cert, tls_key).await?;
    let edge = Edge::new(
        TicketRedeemer::new(&website_origin, service_credential)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_accepts_init_and_the_existing_service_invocation() {
        let init = Cli::try_parse_from(["pontia-edge", "init"]).unwrap();
        assert!(matches!(init.command, Some(Command::Init)));

        let service = Cli::try_parse_from([
            "pontia-edge",
            "--tls-cert",
            "cert.pem",
            "--tls-key",
            "key.pem",
        ])
        .unwrap();
        assert!(service.command.is_none());
        assert_eq!(service.serve.tls_cert, Some(PathBuf::from("cert.pem")));
        assert_eq!(service.serve.tls_key, Some(PathBuf::from("key.pem")));
    }
}
