use std::{
    io::BufReader,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use clap::{Args, Parser, Subcommand};
use pontia_edge::{
    BrowserAccess, BrowserOrigins, ConnectionLimits, Edge, TicketRedeemer,
    acme::{ACCOUNT_PATH, InstantAcmeIssuer, TLS_PATH, issue_and_save},
    challenge::ChallengeServer,
    config::{CONFIG_PATH, DATABASE_PATH, ServiceConfig, hostname_from_tunnel_url},
    credential::{CREDENTIAL_PATH, ensure_managed_directory, ensure_root, read_edge_credential},
    enrollment::{EdgeNetworkClient, HttpCloudClient, InitializationResult, initialize_and_enroll},
    network::routed_public_ipv4,
    systemd::{Systemd, UNIT_PATH},
};
use rustls::{
    RootCertStore,
    client::WebPkiServerVerifier,
    client::danger::ServerCertVerifier,
    pki_types::{ServerName, UnixTime},
};
use uuid::Uuid;

const DNS_WAIT_ATTEMPTS: usize = 60;
const HEALTH_WAIT_ATTEMPTS: usize = 30;
const RETRY_DELAY: Duration = Duration::from_secs(5);
const RENEW_AFTER: Duration = Duration::from_secs(50 * 24 * 60 * 60);

#[derive(Parser)]
#[command(
    about = "Pontia device connection service",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize this edge, configure its public endpoint, and install its service.
    Init(InitArgs),
}

#[derive(Args)]
struct InitArgs {
    #[arg(long)]
    cloud_origin: String,
    #[arg(long, value_parser = parse_edge_id)]
    edge_id: Uuid,
    #[arg(long)]
    ticket: String,
    #[arg(long, action = clap::ArgAction::SetTrue, required = true)]
    agree_to_lets_encrypt_subscriber_agreement: bool,
    #[arg(long, hide = true)]
    acme_staging: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Init(args)) => init(args).await,
        None => serve().await,
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
    ensure_managed_directory()?;
    anyhow::ensure!(
        args.agree_to_lets_encrypt_subscriber_agreement,
        "accept the Let's Encrypt Subscriber Agreement with --agree-to-lets-encrypt-subscriber-agreement"
    );
    let client = HttpCloudClient::new(&args.cloud_origin)?;
    let enrollment = initialize_and_enroll(
        &client,
        CREDENTIAL_PATH.as_ref(),
        args.edge_id,
        &args.ticket,
    )
    .await?;
    let identity = match enrollment {
        InitializationResult::AlreadyRegistered(identity) => {
            println!(
                "Edge {} is available at {}.",
                identity.name, identity.tunnel_url
            );
            return Ok(());
        }
        InitializationResult::Enrolled(identity) => identity,
    };
    let hostname = hostname_from_tunnel_url(&identity.tunnel_url)?;
    let credential = read_edge_credential(CREDENTIAL_PATH.as_ref())?;
    let candidate_ipv4 = routed_public_ipv4(client.origin()).await?;
    let expected_config = ServiceConfig {
        cloud_origin: args.cloud_origin,
        hostname: hostname.clone(),
        browser_bootstrap_origin: "https://pontia.dev".to_owned(),
        browser_dashboard_origin: "https://app.pontia.dev".to_owned(),
    };
    let reusable_certificate = existing_certificate_matches(&expected_config).await;
    let systemd = Systemd::default();
    let service_already_running = reusable_certificate && systemd.is_active();
    let challenge_server = if service_already_running {
        None
    } else {
        Some(ChallengeServer::start(SocketAddr::from((candidate_ipv4, 80))).await?)
    };

    let configured_hostname = client
        .configure_network(&args.ticket, &credential.value, &candidate_ipv4.to_string())
        .await?;
    anyhow::ensure!(
        configured_hostname == hostname,
        "Cloud configured a different hostname"
    );
    wait_for_dns(&hostname, candidate_ipv4).await?;

    if !reusable_certificate {
        let issuer = if args.acme_staging {
            InstantAcmeIssuer::staging(ACCOUNT_PATH)
        } else {
            InstantAcmeIssuer::production(ACCOUNT_PATH)
        };
        issue_and_save(
            &issuer,
            &hostname,
            challenge_server
                .as_ref()
                .context("HTTP challenge server is unavailable")?
                .responses(),
            Path::new(TLS_PATH),
        )
        .await?;
    }
    expected_config.save(Path::new(CONFIG_PATH))?;
    if let Some(challenge_server) = challenge_server {
        challenge_server.stop().await?;
    }

    systemd.install_and_start(Path::new(UNIT_PATH))?;
    wait_for_health(&client, &args.ticket, &credential.value, &hostname).await?;
    println!(
        "Edge {} is available at {}.",
        identity.name, identity.tunnel_url
    );
    Ok(())
}

async fn existing_certificate_matches(expected: &ServiceConfig) -> bool {
    if ServiceConfig::read(Path::new(CONFIG_PATH)).ok().as_ref() != Some(expected) {
        return false;
    }
    certificate_matches_hostname(Path::new(TLS_PATH), &expected.hostname)
        && RustlsConfig::from_pem_file(TLS_PATH, TLS_PATH)
            .await
            .is_ok()
}

fn certificate_matches_hostname(path: &Path, hostname: &str) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(certificates) =
        rustls_pemfile::certs(&mut BufReader::new(file)).collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    let Some((end_entity, intermediates)) = certificates.split_first() else {
        return false;
    };
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let Ok(verifier) = WebPkiServerVerifier::builder(roots.into()).build() else {
        return false;
    };
    let Ok(server_name) = ServerName::try_from(hostname.to_owned()) else {
        return false;
    };
    verifier
        .verify_server_cert(
            end_entity,
            intermediates,
            &server_name,
            &[],
            UnixTime::now(),
        )
        .is_ok()
}

async fn wait_for_dns(hostname: &str, candidate: Ipv4Addr) -> Result<()> {
    for _ in 0..DNS_WAIT_ATTEMPTS {
        if let Ok(addresses) = tokio::net::lookup_host((hostname, 443)).await
            && addresses
                .into_iter()
                .any(|address| address.ip() == candidate)
        {
            return Ok(());
        }
        tokio::time::sleep(RETRY_DELAY).await;
    }
    anyhow::bail!("assigned hostname did not resolve to the verified IPv4 address")
}

async fn wait_for_health(
    client: &HttpCloudClient,
    ticket: &str,
    credential: &str,
    hostname: &str,
) -> Result<()> {
    for _ in 0..HEALTH_WAIT_ATTEMPTS {
        if matches!(client.verify_health(ticket, credential).await, Ok(value) if value == hostname)
        {
            return Ok(());
        }
        tokio::time::sleep(RETRY_DELAY).await;
    }
    anyhow::bail!("Cloud could not verify the public HTTPS health endpoint")
}

async fn serve() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pontia=info".into()),
        )
        .init();
    let config = ServiceConfig::read(Path::new(CONFIG_PATH))?;
    let service_credential = read_edge_credential(CREDENTIAL_PATH.as_ref())?.value;
    let tls = RustlsConfig::from_pem_file(TLS_PATH, TLS_PATH).await?;
    let challenge_server = ChallengeServer::start("0.0.0.0:80".parse().unwrap()).await?;
    let access = BrowserAccess::open(Path::new(DATABASE_PATH)).await?;
    let origins = BrowserOrigins {
        bootstrap: config.browser_bootstrap_origin.clone(),
        dashboard: config.browser_dashboard_origin.clone(),
    };
    let edge = Edge::new(
        TicketRedeemer::new(&config.cloud_origin, service_credential)?,
        access,
        origins,
        ConnectionLimits::default(),
    );
    let handle = axum_server::Handle::<SocketAddr>::new();
    let signal_edge = edge.clone();
    let signal_handle = handle.clone();
    let signal = tokio::spawn(async move {
        shutdown_signal().await;
        signal_edge.shutdown();
        signal_handle.graceful_shutdown(Some(Duration::from_secs(5)));
    });
    let renewal = tokio::spawn(renew_certificates(
        config,
        challenge_server.responses(),
        tls.clone(),
    ));
    tracing::info!(addr = "0.0.0.0:443", "starting edge");
    let result = axum_server::bind_rustls("0.0.0.0:443".parse().unwrap(), tls)
        .handle(handle)
        .serve(edge.router().into_make_service())
        .await;
    edge.shutdown();
    signal.abort();
    renewal.abort();
    challenge_server.stop().await?;
    result?;
    Ok(())
}

async fn renew_certificates(
    config: ServiceConfig,
    challenges: pontia_edge::challenge::ChallengeResponses,
    tls: RustlsConfig,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        if !certificate_needs_renewal(Path::new(TLS_PATH)) {
            continue;
        }
        let issuer = InstantAcmeIssuer::production(ACCOUNT_PATH);
        let result = async {
            issue_and_save(
                &issuer,
                &config.hostname,
                challenges.clone(),
                Path::new(TLS_PATH),
            )
            .await?;
            tls.reload_from_pem_file(TLS_PATH, TLS_PATH).await?;
            Result::<()>::Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "certificate renewal failed");
        } else {
            tracing::info!(hostname = %config.hostname, "certificate renewed");
        }
    }
}

fn certificate_needs_renewal(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_none_or(|age| age >= RENEW_AFTER)
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
    fn rejects_an_untrusted_certificate_even_for_the_assigned_hostname() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("tls.pem");
        let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(vec![
            "brave-silver-atlas.edge.pontia.dev".to_owned(),
        ])
        .unwrap();
        std::fs::write(
            &path,
            format!("{}{}", cert.pem(), signing_key.serialize_pem()),
        )
        .unwrap();

        assert!(!certificate_matches_hostname(
            &path,
            "brave-silver-atlas.edge.pontia.dev"
        ));
        assert!(!certificate_matches_hostname(
            &path,
            "silent-crimson-orion.edge.pontia.dev"
        ));
    }

    #[test]
    fn command_line_requires_explicit_subscriber_agreement() {
        let init = Cli::try_parse_from([
            "pontia-edge",
            "init",
            "--cloud-origin",
            "https://pontia.example",
            "--edge-id",
            "0199791c-6600-7000-8000-000000000001",
            "--ticket",
            "pet_v1_example",
            "--agree-to-lets-encrypt-subscriber-agreement",
        ])
        .unwrap();
        assert!(matches!(init.command, Some(Command::Init(_))));
        assert!(
            Cli::try_parse_from([
                "pontia-edge",
                "init",
                "--cloud-origin",
                "https://pontia.example",
                "--edge-id",
                "0199791c-6600-7000-8000-000000000001",
                "--ticket",
                "pet_v1_example",
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["pontia-edge"]).is_ok());
    }
}
