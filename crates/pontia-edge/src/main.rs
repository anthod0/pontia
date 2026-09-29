use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use clap::{Args, Parser, Subcommand};
use pontia_edge::{
    ConnectionLimits, Edge, TicketRedeemer,
    credential::{CREDENTIAL_PATH, ensure_root, read_edge_credential},
    enrollment::{HttpWebsiteClient, InitializationResult, initialize_and_enroll},
};
use uuid::Uuid;

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
    /// Initialize this edge's service credential and register it with Pontia.
    Init(InitArgs),
}

#[derive(Args)]
struct InitArgs {
    #[arg(long)]
    website_origin: String,
    #[arg(long, value_parser = parse_edge_id)]
    edge_id: Uuid,
    #[arg(long)]
    ticket: String,
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
        Some(Command::Init(args)) => init(args).await,
        None => serve(cli.serve).await,
    }
}

fn parse_edge_id(value: &str) -> std::result::Result<Uuid, String> {
    let id = Uuid::parse_str(value).map_err(|_| "edge ID must be a UUID v7".to_owned())?;
    if id.get_version_num() != 7 || id.to_string() != value {
        return Err("edge ID must be a canonical UUID v7".to_owned());
    }
    Ok(id)
}

async fn init(args: InitArgs) -> Result<()> {
    ensure_root(rustix::process::geteuid().as_raw())?;
    let client = HttpWebsiteClient::new(&args.website_origin)?;
    let result = initialize_and_enroll(
        &client,
        CREDENTIAL_PATH.as_ref(),
        args.edge_id,
        &args.ticket,
    )
    .await?;
    match result {
        InitializationResult::AlreadyRegistered(identity) => {
            println!("Edge {} is already registered.", identity.name);
        }
        InitializationResult::Enrolled(identity) => {
            println!("Edge {} registered successfully.", identity.name);
        }
    }
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
    let service_credential = read_edge_credential(CREDENTIAL_PATH.as_ref())?.value;
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
        let init = Cli::try_parse_from([
            "pontia-edge",
            "init",
            "--website-origin",
            "https://pontia.example",
            "--edge-id",
            "0199791c-6600-7000-8000-000000000001",
            "--ticket",
            "pet_v1_example",
        ])
        .unwrap();
        assert!(matches!(init.command, Some(Command::Init(_))));

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
