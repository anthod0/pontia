use std::{
    io::{BufReader, Write},
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use clap::{Args, Parser, Subcommand};
use pontia_edge::{
    BrowserAccess, BrowserOrigins, ConnectionLimits, Edge, TicketRedeemer,
    acme::{
        ACCOUNT_PATH, AcmeChallenge, CloudDnsChallenges, InstantAcmeIssuer, TLS_PATH,
        complete_dns_and_save, issue_dns_and_save, issue_http_and_save,
    },
    challenge::ChallengeServer,
    config::{CONFIG_PATH, DATABASE_PATH, ServiceConfig, hostname_from_tunnel_url},
    credential::{CREDENTIAL_PATH, ensure_managed_directory, ensure_root, read_edge_credential},
    enrollment::{EdgeNetworkClient, HttpCloudClient, InitializationResult, initialize_and_enroll},
    network::routed_public_ipv4,
    port::{parse_edge_port, tunnel_url},
    systemd::{Systemd, UNIT_PATH},
    tls::AcmeAcceptor,
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
    /// Public HTTPS/WSS port (recommended alternative: 8443). Also used for IP verification.
    #[arg(long, value_parser = parse_edge_port)]
    port: Option<u16>,
    /// ACME validation method. HTTP-01 always requires public port 80.
    #[arg(long, value_enum, default_value = "http-01")]
    acme_challenge: AcmeChallenge,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Init(args)) => tokio::select! {
            result = init(args) => result,
            _ = shutdown_signal() => anyhow::bail!("edge initialization interrupted"),
        },
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
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    init_with_output(args, &mut output).await
}

async fn init_with_output<W: Write + ?Sized>(args: InitArgs, output: &mut W) -> Result<()> {
    status(output, "Checking local requirements...")?;
    ensure_root(rustix::process::geteuid().as_raw())?;
    ensure_managed_directory()?;
    anyhow::ensure!(
        args.agree_to_lets_encrypt_subscriber_agreement,
        "accept the Let's Encrypt Subscriber Agreement with --agree-to-lets-encrypt-subscriber-agreement"
    );

    status(output, "Preparing edge deployment with Pontia Cloud...")?;
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
            status(
                output,
                &format!(
                    "Edge {} is already registered and available at {}.",
                    identity.name, identity.tunnel_url
                ),
            )?;
            return Ok(());
        }
        InitializationResult::Enrolled(identity) => identity,
    };
    let hostname = hostname_from_tunnel_url(&identity.tunnel_url)?;
    status(
        output,
        &format!("Deployment authorized for {} ({hostname}).", identity.name),
    )?;

    let credential = read_edge_credential(CREDENTIAL_PATH.as_ref())?;
    status(output, "Detecting the public IPv4 address...")?;
    let candidate_ipv4 = routed_public_ipv4(client.origin()).await?;
    status(output, &format!("Public IPv4 address: {candidate_ipv4}"))?;

    let expected_config = ServiceConfig {
        cloud_origin: args.cloud_origin,
        hostname: hostname.clone(),
        port: args.port.unwrap_or(443),
        acme_challenge: args.acme_challenge,
        browser_bootstrap_origin: "https://pontia.dev".to_owned(),
        browser_dashboard_origin: "https://app.pontia.dev".to_owned(),
    };
    let reusable_certificate = existing_certificate_matches(&expected_config).await;
    let systemd = Systemd::default();
    let probe_port = args.port.unwrap_or(80);
    status(
        output,
        &format!("Starting the IP challenge server on {candidate_ipv4}:{probe_port}..."),
    )?;
    let challenge_server =
        ChallengeServer::start(SocketAddr::from((candidate_ipv4, probe_port))).await?;
    let issuer = if args.acme_staging {
        InstantAcmeIssuer::staging(ACCOUNT_PATH)
    } else {
        InstantAcmeIssuer::production(ACCOUNT_PATH)
    };
    let dns_order = if !reusable_certificate && args.acme_challenge == AcmeChallenge::Dns01 {
        Some(issuer.prepare_dns(&hostname).await?)
    } else {
        None
    };
    let dns_value = dns_order.as_ref().and_then(|order| order.value.clone());

    status(output, &format!("Configuring DNS for {hostname}..."))?;
    let configured_hostname = client
        .configure_network(
            &args.ticket,
            &credential.value,
            &candidate_ipv4.to_string(),
            probe_port,
            dns_value.as_deref(),
        )
        .await?;
    anyhow::ensure!(
        configured_hostname == hostname,
        "Cloud configured a different hostname: expected {hostname}, received {configured_hostname}"
    );
    // The temporary plaintext listener belongs only to IP verification.
    challenge_server.stop().await?;
    status(output, "DNS configuration accepted by Pontia Cloud.")?;

    if !reusable_certificate {
        status(
            output,
            &format!("Requesting a TLS certificate for {hostname} from Let's Encrypt..."),
        )?;
        if let Some(order) = dns_order {
            let dns = CloudDnsChallenges {
                client: &client,
                credential: &credential.value,
                ticket: Some(&args.ticket),
            };
            complete_dns_and_save(order, &hostname, Path::new(TLS_PATH), &dns).await?;
        } else {
            wait_for_dns(&hostname, candidate_ipv4, output).await?;
            issue_http_and_save(&issuer, &hostname, Path::new(TLS_PATH), None).await?;
        }
        status(output, "TLS certificate issued and saved.")?;
    }

    status(output, "Saving edge configuration...")?;
    expected_config.save(Path::new(CONFIG_PATH))?;
    status(output, "Installing and starting the pontia-edge service...")?;
    systemd.install_and_start(Path::new(UNIT_PATH))?;
    status(output, "pontia-edge service started.")?;
    wait_for_health(
        &client,
        &args.ticket,
        &credential.value,
        (&hostname, expected_config.port),
        output,
        HEALTH_WAIT_ATTEMPTS,
        RETRY_DELAY,
    )
    .await?;
    status(
        output,
        &format!(
            "Edge {} is available at {}.",
            identity.name,
            tunnel_url(&hostname, expected_config.port)
        ),
    )?;
    Ok(())
}

fn status(output: &mut (impl Write + ?Sized), message: &str) -> Result<()> {
    writeln!(output, "{message}").context("failed to write deployment progress")?;
    output
        .flush()
        .context("failed to flush deployment progress")
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

async fn wait_for_dns(
    hostname: &str,
    candidate: Ipv4Addr,
    output: &mut (impl Write + ?Sized),
) -> Result<()> {
    status(
        output,
        &format!(
            "Waiting for {hostname} to resolve to {candidate} (up to {})...",
            format_duration(RETRY_DELAY * DNS_WAIT_ATTEMPTS as u32)
        ),
    )?;
    let mut last_observation = "no DNS response received".to_owned();
    for attempt in 1..=DNS_WAIT_ATTEMPTS {
        match tokio::net::lookup_host((hostname, 443)).await {
            Ok(addresses) => {
                let mut addresses = addresses.map(|address| address.ip()).collect::<Vec<_>>();
                addresses.sort_unstable();
                addresses.dedup();
                if addresses.iter().any(|address| *address == candidate) {
                    status(
                        output,
                        &format!("DNS now resolves {hostname} to {candidate}."),
                    )?;
                    return Ok(());
                }
                last_observation = if addresses.is_empty() {
                    "the lookup returned no addresses".to_owned()
                } else {
                    format!(
                        "resolved to {} instead of {candidate}",
                        addresses
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
            }
            Err(error) => last_observation = format!("DNS lookup failed: {error}"),
        }
        tokio::time::sleep(RETRY_DELAY).await;
        if attempt % 6 == 0 && attempt < DNS_WAIT_ATTEMPTS {
            status(
                output,
                &format!(
                    "Still waiting for DNS after {}; last check: {last_observation}.",
                    format_duration(RETRY_DELAY * attempt as u32)
                ),
            )?;
        }
    }
    anyhow::bail!(
        "assigned hostname did not resolve to the verified IPv4 address; last DNS check: {last_observation}"
    )
}

async fn wait_for_health<C: EdgeNetworkClient>(
    client: &C,
    ticket: &str,
    credential: &str,
    endpoint: (&str, u16),
    output: &mut (impl Write + ?Sized),
    attempts: usize,
    retry_delay: Duration,
) -> Result<()> {
    status(
        output,
        &format!(
            "Waiting for Pontia Cloud to verify the public HTTPS endpoint (up to {})...",
            format_duration(retry_delay * attempts as u32)
        ),
    )?;
    let (hostname, port) = endpoint;
    let mut last_observation = "no health response received".to_owned();
    for attempt in 1..=attempts {
        match client.verify_health(ticket, credential, port).await {
            Ok(value) if value == hostname => {
                status(output, "Public HTTPS health verification succeeded.")?;
                return Ok(());
            }
            Ok(value) => {
                last_observation = format!("Cloud returned hostname {value} instead of {hostname}");
            }
            Err(error) => last_observation = format!("{error:#}"),
        }
        tokio::time::sleep(retry_delay).await;
        if attempt % 6 == 0 && attempt < attempts {
            status(
                output,
                &format!(
                    "Still waiting for HTTPS health verification after {}; last check: {last_observation}.",
                    format_duration(retry_delay * attempt as u32)
                ),
            )?;
        }
    }
    anyhow::bail!(
        "Cloud could not verify the public HTTPS health endpoint; last health check: {last_observation}"
    )
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    match (minutes, remaining_seconds) {
        (0, seconds) => format!("{seconds} seconds"),
        (minutes, 0) => format!("{minutes} minutes"),
        (minutes, seconds) => format!("{minutes} minutes {seconds} seconds"),
    }
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
    let access = BrowserAccess::open(Path::new(DATABASE_PATH)).await?;
    let origins = BrowserOrigins {
        bootstrap: config.browser_bootstrap_origin.clone(),
        dashboard: config.browser_dashboard_origin.clone(),
    };
    let edge = Edge::new(
        TicketRedeemer::new(&config.cloud_origin, service_credential.clone())?,
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
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let shared_acme = (config.port == 80 && config.acme_challenge == AcmeChallenge::Http01)
        .then(pontia_edge::challenge::ChallengeResponses::default);
    let renewal = tokio::spawn(renew_certificates(
        config,
        service_credential,
        tls.clone(),
        shared_acme.clone(),
    ));
    tracing::info!(%address, "starting edge");
    let result = if let Some(responses) = shared_acme {
        axum_server::bind(address)
            .acceptor(AcmeAcceptor::new(tls, responses))
            .handle(handle)
            .serve(edge.router().into_make_service())
            .await
    } else {
        axum_server::bind_rustls(address, tls)
            .handle(handle)
            .serve(edge.router().into_make_service())
            .await
    };
    edge.shutdown();
    signal.abort();
    renewal.abort();
    result?;
    Ok(())
}

async fn renew_certificates(
    config: ServiceConfig,
    credential: String,
    tls: RustlsConfig,
    shared_acme: Option<pontia_edge::challenge::ChallengeResponses>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        if !certificate_needs_renewal(Path::new(TLS_PATH)) {
            continue;
        }
        let issuer = InstantAcmeIssuer::production(ACCOUNT_PATH);
        let result = async {
            if config.acme_challenge == AcmeChallenge::Dns01 {
                let client = HttpCloudClient::new(&config.cloud_origin)?;
                let dns = CloudDnsChallenges {
                    client: &client,
                    credential: &credential,
                    ticket: None,
                };
                issue_dns_and_save(&issuer, &config.hostname, Path::new(TLS_PATH), &dns).await?;
            } else {
                issue_http_and_save(
                    &issuer,
                    &config.hostname,
                    Path::new(TLS_PATH),
                    shared_acme.clone(),
                )
                .await?;
            }
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

    struct FailingHealthClient;

    impl EdgeNetworkClient for FailingHealthClient {
        async fn configure_network(
            &self,
            _ticket: &str,
            _credential: &str,
            _candidate_ipv4: &str,
            _port: u16,
            _dns_challenge: Option<&str>,
        ) -> Result<String> {
            unreachable!()
        }

        async fn verify_health(
            &self,
            _ticket: &str,
            _credential: &str,
            _port: u16,
        ) -> Result<String> {
            anyhow::bail!(
                "Cloud could not verify public edge health (HTTP 502 Bad Gateway, error: health_verification_failed)"
            )
        }
    }

    #[tokio::test]
    async fn health_timeout_reports_progress_and_the_last_cloud_error() {
        let mut output = Vec::new();
        let error = wait_for_health(
            &FailingHealthClient,
            "secret-ticket",
            "secret-credential",
            ("brave-silver-atlas.edge.pontia.dev", 443),
            &mut output,
            1,
            Duration::ZERO,
        )
        .await
        .unwrap_err()
        .to_string();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("Waiting for Pontia Cloud to verify"));
        assert!(error.contains("HTTP 502 Bad Gateway"));
        assert!(error.contains("health_verification_failed"));
        assert!(!output.contains("secret-ticket"));
        assert!(!output.contains("secret-credential"));
        assert!(!error.contains("secret-ticket"));
        assert!(!error.contains("secret-credential"));
    }

    #[test]
    fn deployment_wait_durations_are_readable() {
        assert_eq!(
            format_duration(Duration::from_secs(150)),
            "2 minutes 30 seconds"
        );
        assert_eq!(format_duration(Duration::from_secs(300)), "5 minutes");
    }

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
    fn endpoint_port_and_acme_method_are_independent_cli_options() {
        let base = [
            "pontia-edge",
            "init",
            "--cloud-origin",
            "https://pontia.example",
            "--edge-id",
            "0199791c-6600-7000-8000-000000000001",
            "--ticket",
            "ticket",
            "--agree-to-lets-encrypt-subscriber-agreement",
        ];
        for (extra, port, method) in [
            (vec![], None, AcmeChallenge::Http01),
            (vec!["--port", "8443"], Some(8443), AcmeChallenge::Http01),
            (
                vec!["--acme-challenge", "dns-01"],
                None,
                AcmeChallenge::Dns01,
            ),
            (
                vec!["--port", "8443", "--acme-challenge", "dns-01"],
                Some(8443),
                AcmeChallenge::Dns01,
            ),
        ] {
            let cli = Cli::try_parse_from(base.into_iter().chain(extra)).unwrap();
            let Some(Command::Init(args)) = cli.command else {
                panic!("expected init")
            };
            assert_eq!(args.port, port);
            assert_eq!(args.acme_challenge, method);
        }
        for method in ["dns", "http", "DNS-01", "HTTP-01", "tls-alpn-01"] {
            assert!(
                Cli::try_parse_from(base.into_iter().chain(["--acme-challenge", method])).is_err()
            );
        }
        for port in ["0", "25", "6000", "65536"] {
            assert!(Cli::try_parse_from(base.into_iter().chain(["--port", port])).is_err());
        }
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
