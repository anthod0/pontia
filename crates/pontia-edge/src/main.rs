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
    BrowserOrigins, ConnectionLimits, Edge, TicketRedeemer,
    acme::{
        ACCOUNT_PATH, AcmeChallenge, CloudDnsChallenges, InstantAcmeIssuer, TLS_PATH,
        complete_dns_and_save, issue_dns_and_save, issue_http_and_save,
    },
    challenge::ChallengeServer,
    config::{CONFIG_PATH, ServiceConfig, hostname_from_tunnel_url},
    credential::{CREDENTIAL_PATH, ensure_managed_directory, ensure_root, read_edge_credential},
    enrollment::{EdgeNetworkClient, HttpCloudClient, InitializationResult, initialize_and_enroll},
    network::{discover_public_ipv4, system_dns_resolver},
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

mod update;

const DNS_WAIT_TIMEOUT: Duration = Duration::from_secs(300);
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
    /// Update pontia-edge to the latest stable release.
    Update,
}

#[derive(Args)]
struct InitArgs {
    #[arg(long)]
    cloud_origin: String,
    #[arg(long, value_parser = parse_edge_id)]
    edge_id: Uuid,
    #[arg(long)]
    ticket: String,
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
        Some(Command::Init(args)) => run_until_shutdown(init(args)).await,
        Some(Command::Update) => update::run().await,
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
    // Do not hold the global stdout lock across network waits or cancellation.
    let mut output = std::io::stdout();
    init_with_output(args, &mut output).await
}

async fn init_with_output<W: Write + ?Sized>(args: InitArgs, output: &mut W) -> Result<()> {
    status(output, "Checking local requirements...")?;
    ensure_root(rustix::process::geteuid().as_raw())?;
    ensure_managed_directory()?;

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
    let candidate_ipv4 = discover_public_ipv4(client.origin()).await?;
    status(output, &format!("Public IPv4 address: {candidate_ipv4}"))?;

    let expected_config = ServiceConfig {
        cloud_origin: args.cloud_origin,
        hostname: hostname.clone(),
        port: args.port.unwrap_or(443),
        acme_challenge: args.acme_challenge,
        browser_dashboard_origin: "https://app.pontia.dev".to_owned(),
    };
    let reusable_certificate = existing_certificate_matches(&expected_config).await;
    let systemd = Systemd::default();
    let probe_port = args.port.unwrap_or(80);
    status(
        output,
        &format!("Starting the IP challenge server on 0.0.0.0:{probe_port}..."),
    )?;
    let challenge_server =
        ChallengeServer::start(SocketAddr::from((Ipv4Addr::UNSPECIFIED, probe_port))).await?;
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
            complete_dns_and_save(order, &hostname, Path::new(TLS_PATH), &dns, output).await?;
        } else {
            wait_for_dns(&hostname, candidate_ipv4, output).await?;
            issue_http_and_save(&issuer, &hostname, Path::new(TLS_PATH), None, output).await?;
        }
        status(output, "TLS certificate issued and saved.")?;
    }

    status(output, "Saving edge configuration...")?;
    expected_config.save(Path::new(CONFIG_PATH))?;
    status(output, "Installing and starting the pontia-edge service...")?;
    systemd.install_and_start(Path::new(UNIT_PATH)).await?;
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
            "Waiting for {hostname} to resolve to {candidate} using system DNS (up to 5 minutes)..."
        ),
    )?;
    let resolver = system_dns_resolver()?;
    let name = format!("{hostname}.");
    let started = tokio::time::Instant::now();
    let deadline = started + DNS_WAIT_TIMEOUT;
    let mut reports =
        tokio::time::interval_at(started + Duration::from_secs(15), Duration::from_secs(15));
    reports.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut next_query = started;
    let mut last_observation = "no DNS response received".to_owned();
    loop {
        let query_at = next_query;
        let query = async {
            tokio::time::sleep_until(query_at).await;
            tokio::time::timeout(Duration::from_secs(5), resolver.ipv4_lookup(name.as_str())).await
        };
        tokio::pin!(query);
        let result = loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline) => {
                    anyhow::bail!("assigned hostname did not resolve to the verified IPv4 address within 5 minutes; last DNS check: {last_observation}");
                }
                result = &mut query => break result,
                _ = reports.tick() => {
                    status(output, &format!("Still waiting for DNS after {} seconds; last check: {last_observation}.", started.elapsed().as_secs()))?;
                }
            }
        };
        match result {
            Ok(Ok(addresses)) if addresses.iter().any(|address| address.0 == candidate) => {
                status(
                    output,
                    &format!("DNS now resolves {hostname} to {candidate}."),
                )?;
                return Ok(());
            }
            Ok(Ok(addresses)) => {
                last_observation = format!(
                    "resolved to {} instead of {candidate}",
                    addresses
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(Err(error)) => last_observation = format!("system DNS lookup failed: {error}"),
            Err(_) => last_observation = "system DNS lookup timed out after 5 seconds".to_owned(),
        }
        next_query = tokio::time::Instant::now() + RETRY_DELAY;
    }
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
    let origins = BrowserOrigins {
        dashboard: config.browser_dashboard_origin.clone(),
    };
    let edge = Edge::new(
        TicketRedeemer::new(&config.cloud_origin, service_credential.clone())?,
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
                issue_dns_and_save(
                    &issuer,
                    &config.hostname,
                    Path::new(TLS_PATH),
                    &dns,
                    &mut std::io::stdout(),
                )
                .await?;
            } else {
                issue_http_and_save(
                    &issuer,
                    &config.hostname,
                    Path::new(TLS_PATH),
                    shared_acme.clone(),
                    &mut std::io::stdout(),
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

struct ShutdownSignals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
}

impl ShutdownSignals {
    fn install() -> Result<Self> {
        Ok(Self {
            #[cfg(unix)]
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .context("failed to install SIGINT handler")?,
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("failed to install SIGTERM handler")?,
        })
    }

    async fn recv(&mut self) {
        #[cfg(unix)]
        tokio::select! {
            _ = self.interrupt.recv() => {}
            _ = self.terminate.recv() => {}
        }
        #[cfg(not(unix))]
        tokio::signal::ctrl_c().await.expect("wait for Ctrl-C");
    }
}

async fn run_until_shutdown(
    operation: impl std::future::Future<Output = Result<()>>,
) -> Result<()> {
    // Register both handlers before polling initialization, and prioritize cancellation.
    let mut signals = ShutdownSignals::install()?;
    tokio::select! {
        biased;
        _ = signals.recv() => anyhow::bail!("edge initialization interrupted"),
        result = operation => result,
    }
}

async fn shutdown_signal() {
    ShutdownSignals::install()
        .expect("install shutdown handlers")
        .recv()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Run the real signal listener in an isolated process, never signal the test runner.
    #[cfg(unix)]
    #[tokio::test]
    async fn signal_test_child() {
        let Some(ready_path) = std::env::var_os("PONTIA_EDGE_SIGNAL_TEST_READY") else {
            return;
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let result = run_until_shutdown(async {
            std::fs::write(ready_path, address.to_string()).unwrap();
            // Simulate an initialization request whose peer never responds.
            reqwest::Client::new()
                .get(format!("http://{address}"))
                .send()
                .await?;
            anyhow::bail!("request unexpectedly completed")
        })
        .await;
        assert_eq!(
            result.unwrap_err().to_string(),
            "edge initialization interrupted"
        );
    }

    #[cfg(unix)]
    async fn assert_signal_cancels_initialization(
        signal: rustix::process::Signal,
        ignore_sigint: bool,
    ) {
        let root = tempfile::tempdir().unwrap();
        let ready = root.path().join("ready");
        let executable = std::env::current_exe().unwrap();
        let mut command = if ignore_sigint {
            // Reproduce SIGINT being inherited as ignored from a launching shell.
            let mut command = tokio::process::Command::new("sh");
            command.args(["-c", "trap '' INT; exec \"$@\"", "sh"]);
            command.arg(&executable);
            command
        } else {
            tokio::process::Command::new(&executable)
        };
        let mut child = command
            .args(["--exact", "tests::signal_test_child", "--nocapture"])
            .env("PONTIA_EDGE_SIGNAL_TEST_READY", &ready)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("signal test exited before installing handlers: {status}");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("signal test did not become ready");
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(child.id().unwrap() as i32).unwrap(),
            signal,
        )
        .unwrap();
        let status = tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .expect("initialization did not respond to signal")
            .unwrap();
        assert!(status.success(), "signal test failed: {status}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sigint_cancels_stalled_initialization_even_when_inherited_as_ignored() {
        assert_signal_cancels_initialization(rustix::process::Signal::INT, false).await;
        assert_signal_cancels_initialization(rustix::process::Signal::INT, true).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sigterm_cancels_stalled_initialization() {
        assert_signal_cancels_initialization(rustix::process::Signal::TERM, false).await;
    }

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
    fn update_is_a_standalone_command() {
        let cli = Cli::try_parse_from(["pontia-edge", "update"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Update)));
        assert!(Cli::try_parse_from(["pontia-edge", "update", "--ticket", "secret"]).is_err());
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
    fn command_line_initializes_without_subscriber_agreement_flag() {
        let init = Cli::try_parse_from([
            "pontia-edge",
            "init",
            "--cloud-origin",
            "https://pontia.example",
            "--edge-id",
            "0199791c-6600-7000-8000-000000000001",
            "--ticket",
            "pet_v1_example",
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
                "--agree-to-lets-encrypt-subscriber-agreement",
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["pontia-edge"]).is_ok());
    }
}
