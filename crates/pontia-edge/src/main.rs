use std::{
    io::{BufReader, Write},
    net::SocketAddr,
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use clap::{Args, Parser, Subcommand};
use pontia_edge::{
    AccessTokenVerifier, BrowserOrigins, ConnectionLimits, Edge, TicketRedeemer,
    acme::{
        ACCOUNT_PATH, AcmeChallenge, CloudDnsChallenges, InstantAcmeIssuer, TLS_PATH,
        issue_dns_and_save, issue_http_and_save,
    },
    certificate,
    config::{
        CLOUD_ORIGIN, CONFIG_PATH, PortPreviewConfig, PortPreviewSetting, ServiceConfig,
        hostname_from_tunnel_url,
    },
    credential::{CREDENTIAL_PATH, ensure_managed_directory, ensure_root, read_edge_credential},
    enrollment::{
        EdgeConfirmationClient, HttpCloudClient, InitializationResult, initialize_and_enroll,
    },
    network::{bind_listener, public_addresses},
    pending_init::{self, PENDING_INIT_PATH, PendingInit},
    port::{parse_edge_port, tunnel_url},
    systemd::{Systemd, UNIT_DIRECTORY, UNIT_PATH},
    tls::{AcmeAcceptor, EdgeCertResolver},
};
use rustls::{
    RootCertStore,
    client::WebPkiServerVerifier,
    client::danger::ServerCertVerifier,
    pki_types::{ServerName, UnixTime},
};
use uuid::Uuid;

mod auto_update;
mod update;

const CONFIRMATION_ATTEMPTS: usize = 30;
const RETRY_DELAY: Duration = Duration::from_secs(5);
const FIRST_RENEWAL_CHECK: Duration = Duration::from_secs(60);
const RENEWAL_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Parser)]
#[command(
    about = "Pontia device connection service",
    version = pontia_version::version(),
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize this edge endpoint, obtain a certificate, and install its service.
    Init(InitArgs),
    /// Retry the last incomplete initialization.
    Retry,
    /// Update pontia-edge to the latest stable release.
    Update {
        /// Skip updates while the Edge service is stopped.
        #[arg(long, hide = true)]
        automatic: bool,
    },
    /// Manage automatic stable-release updates.
    AutoUpdate {
        #[command(subcommand)]
        command: auto_update::Command,
    },
}

#[derive(Args)]
struct InitArgs {
    #[arg(long, value_parser = parse_edge_id)]
    edge_id: Uuid,
    #[arg(long)]
    ticket: String,
    #[arg(long, hide = true)]
    acme_staging: bool,
    /// Public HTTPS/WSS port (default 443). Certificate validation also uses public port 80.
    #[arg(long, value_parser = parse_edge_port)]
    port: Option<u16>,
    /// Preview listener ranges, for example 20000-20099 or 3000-9999,20000-20099; use off to disable.
    #[arg(long, value_parser = parse_preview_ports)]
    preview_ports: Option<String>,
}

fn parse_preview_ports(value: &str) -> std::result::Result<String, String> {
    PortPreviewSetting::parse(value).map(|_| value.to_owned())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Init(args)) => run_until_shutdown(init(args)).await,
        Some(Command::Retry) => run_until_shutdown(retry()).await,
        Some(Command::Update { automatic }) => update::run(automatic).await,
        Some(Command::AutoUpdate { command }) => auto_update::run(command).await,
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

impl From<InitArgs> for PendingInit {
    fn from(args: InitArgs) -> Self {
        Self {
            edge_id: args.edge_id,
            ticket: args.ticket,
            acme_staging: args.acme_staging,
            port: args.port,
            preview_ports: args.preview_ports,
        }
    }
}

async fn init(args: InitArgs) -> Result<()> {
    if let Some(value) = args.preview_ports.as_deref()
        && let Some(preview) = PortPreviewSetting::parse(value)
            .map_err(anyhow::Error::msg)?
            .enabled()
    {
        preview
            .allowed_ports(args.port.unwrap_or(443))
            .map_err(anyhow::Error::msg)?;
    }
    // Do not hold the global stdout lock across network waits or cancellation.
    let mut output = std::io::stdout();
    prepare_initialization(&mut output)?;
    let pending = PendingInit::from(args);
    pending.save(Path::new(PENDING_INIT_PATH))?;
    run_pending_initialization(pending, &mut output).await
}

async fn retry() -> Result<()> {
    let mut output = std::io::stdout();
    prepare_initialization(&mut output)?;
    let pending = PendingInit::read(Path::new(PENDING_INIT_PATH))?;
    status(&mut output, "Retrying the pending edge initialization...")?;
    run_pending_initialization(pending, &mut output).await
}

fn prepare_initialization(output: &mut (impl Write + ?Sized)) -> Result<()> {
    status(output, "Checking local requirements...")?;
    ensure_root(rustix::process::geteuid().as_raw())?;
    ensure_managed_directory()
}

async fn run_pending_initialization<W: Write + ?Sized>(
    args: PendingInit,
    output: &mut W,
) -> Result<()> {
    complete_pending_initialization(Path::new(PENDING_INIT_PATH), init_with_output(args, output))
        .await
}

async fn complete_pending_initialization(
    path: &Path,
    operation: impl std::future::Future<Output = Result<()>>,
) -> Result<()> {
    operation.await?;
    pending_init::remove(path)
}

async fn init_with_output<W: Write + ?Sized>(args: PendingInit, output: &mut W) -> Result<()> {
    status(output, "Preparing edge deployment with Pontia Cloud...")?;
    let client = HttpCloudClient::new(CLOUD_ORIGIN)?;
    let enrollment = initialize_and_enroll(
        &client,
        CREDENTIAL_PATH.as_ref(),
        args.edge_id,
        &args.ticket,
    )
    .await?;
    let (identity, already_registered) = match enrollment {
        InitializationResult::AlreadyRegistered(identity) => (identity, true),
        InitializationResult::Enrolled(identity) => (identity, false),
    };
    anyhow::ensure!(
        identity.edge_id == args.edge_id,
        "Cloud returned a different edge identity"
    );
    let hostname = hostname_from_tunnel_url(&identity.tunnel_url)?;
    let expected_config = ServiceConfig {
        hostname: hostname.clone(),
        port: args.port.unwrap_or(443),
        acme_challenge: AcmeChallenge::Http01,
        browser_dashboard_origin: "https://app.pontia.dev".to_owned(),
        preview_base_hostname: None,
        port_preview: match args.preview_ports.as_deref() {
            Some(value) => PortPreviewSetting::parse(value).map_err(anyhow::Error::msg)?,
            None if hostname.parse::<std::net::IpAddr>().is_ok() => {
                PortPreviewSetting::Enabled(PortPreviewConfig::default_pool())
            }
            None => PortPreviewSetting::Disabled,
        },
    };
    if let Some(preview) = expected_config.port_preview.enabled() {
        preview
            .allowed_ports(expected_config.port)
            .map_err(anyhow::Error::msg)?;
    }
    let credential = read_edge_credential(CREDENTIAL_PATH.as_ref())?;
    let systemd = Systemd::default();
    let endpoint = tunnel_url(&hostname, expected_config.port);

    if already_registered {
        anyhow::ensure!(
            identity.tunnel_url == endpoint,
            "registered edge endpoint does not match the pending initialization"
        );
        status(
            output,
            &format!("Edge {hostname} is already registered; completing local setup."),
        )?;
        anyhow::ensure!(
            saved_certificate_is_usable(Path::new(TLS_PATH), &hostname).await,
            "registered edge TLS certificate is missing or invalid"
        );
    } else {
        status(output, &format!("Deployment authorized for {hostname}."))?;
        if expected_config.port != 80 {
            status(
                output,
                "Let's Encrypt HTTP-01 validation requires public TCP port 80 to reach this server.",
            )?;
        }
        let addresses = if hostname.parse::<std::net::IpAddr>().is_ok() {
            status(output, &format!("Checking public IP address {hostname}..."))?;
            public_addresses(&hostname).await?
        } else {
            status(output, &format!("Checking public DNS for {hostname}..."))?;
            let addresses = public_addresses(&hostname).await.with_context(|| {
                format!(
                    "point an A or AAAA record for {hostname} to this server's public IP address, then run `sudo pontia-edge retry`"
                )
            })?;
            status(
                output,
                &format!(
                    "{hostname} resolves to {}.",
                    addresses
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )?;
            addresses
        };
        debug_assert!(!addresses.is_empty());
        if reusable_certificate(Path::new(TLS_PATH), &hostname).await {
            status(output, "Reusing the saved TLS certificate.")?;
        } else {
            status(
                output,
                &format!("Requesting a TLS certificate for {hostname} from Let's Encrypt..."),
            )?;
            let issuer = if args.acme_staging {
                InstantAcmeIssuer::staging(ACCOUNT_PATH)
            } else {
                InstantAcmeIssuer::production(ACCOUNT_PATH)
            };
            issue_http_and_save(&issuer, &hostname, Path::new(TLS_PATH), None, output).await?;
            status(output, "TLS certificate issued and saved.")?;
        }
    }

    status(output, "Saving edge configuration...")?;
    expected_config.save(Path::new(CONFIG_PATH))?;
    status(output, "Installing and starting the pontia-edge service...")?;
    systemd.install_and_start(Path::new(UNIT_PATH)).await?;
    status(output, "pontia-edge service started.")?;
    wait_for_confirmation(
        &client,
        &args.ticket,
        &credential.value,
        &hostname,
        expected_config.port,
        output,
        CONFIRMATION_ATTEMPTS,
        RETRY_DELAY,
    )
    .await?;
    status(output, "Enabling automatic stable-release updates...")?;
    systemd
        .enable_auto_update(Path::new(UNIT_DIRECTORY))
        .await?;
    status(
        output,
        &format!("Edge {hostname} is available at {endpoint}."),
    )?;
    status(
        output,
        "Manage this Edge at https://pontia.dev/settings/edges",
    )?;
    Ok(())
}

fn status(output: &mut (impl Write + ?Sized), message: &str) -> Result<()> {
    writeln!(output, "{message}").context("failed to write deployment progress")?;
    output
        .flush()
        .context("failed to flush deployment progress")
}

/// A saved certificate is reused only while it is trusted and outside its renewal window.
async fn reusable_certificate(path: &Path, hostname: &str) -> bool {
    !certificate::needs_renewal(path, SystemTime::now())
        && saved_certificate_is_usable(path, hostname).await
}

async fn saved_certificate_is_usable(path: &Path, hostname: &str) -> bool {
    certificate_matches_hostname(path, hostname)
        && RustlsConfig::from_pem_file(path, path).await.is_ok()
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

#[allow(clippy::too_many_arguments)]
async fn wait_for_confirmation<C: EdgeConfirmationClient>(
    client: &C,
    ticket: &str,
    credential: &str,
    hostname: &str,
    port: u16,
    output: &mut (impl Write + ?Sized),
    attempts: usize,
    retry_delay: Duration,
) -> Result<()> {
    status(
        output,
        &format!(
            "Waiting for Pontia Cloud to verify endpoint ownership over HTTPS (up to {})...",
            format_duration(retry_delay * attempts as u32)
        ),
    )?;
    let mut last_observation = "no confirmation response received".to_owned();
    for attempt in 1..=attempts {
        match client.confirm(ticket, credential, port).await {
            Ok(value) if value == hostname => {
                status(output, "Pontia Cloud verified and registered this edge.")?;
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
                    "Still waiting for ownership verification after {}; last check: {last_observation}.",
                    format_duration(retry_delay * attempt as u32)
                ),
            )?;
        }
    }
    anyhow::bail!("Cloud could not verify this edge over HTTPS; last check: {last_observation}")
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
    let credential = read_edge_credential(CREDENTIAL_PATH.as_ref())?;
    let access_tokens = AccessTokenVerifier::embedded(credential.edge_id)?;
    let service_credential = credential.value;
    let tls = RustlsConfig::from_pem_file(TLS_PATH, TLS_PATH).await?;
    let (server_tls, preview) = match config.preview_base_hostname.clone() {
        Some(base) => {
            let resolver = EdgeCertResolver::new(&base, tls.clone());
            let preview_path = Path::new("/etc/pontia/edge/preview-tls.pem");
            if preview_path.exists()
                && let Err(error) = resolver.load_preview(preview_path).await
            {
                tracing::warn!(%error,"could not load preview certificate");
            }
            let server_tls = resolver.server();
            let preview = tokio::spawn(maintain_preview(
                base,
                service_credential.clone(),
                resolver,
                preview_path,
            ));
            (server_tls, Some(preview))
        }
        None => (tls.clone(), None),
    };
    let origins = BrowserOrigins {
        dashboard: config.browser_dashboard_origin.clone(),
    };
    let edge = Edge::new(
        TicketRedeemer::new(CLOUD_ORIGIN, service_credential.clone())?,
        access_tokens,
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
    let listener = bind_listener(config.port)?;
    let address = listener.local_addr()?;
    let shared_acme = (config.port == 80 && config.acme_challenge == AcmeChallenge::Http01)
        .then(pontia_edge::challenge::ChallengeResponses::default);
    let router = edge.router_with_port_preview(
        &config.hostname,
        config.port,
        config.preview_base_hostname.as_deref(),
        config.port_preview.enabled(),
        tls.clone(),
    );
    let renewal = tokio::spawn(renew_certificates(
        config,
        service_credential,
        tls.clone(),
        shared_acme.clone(),
    ));
    tracing::info!(%address, "starting edge");
    let result = if let Some(responses) = shared_acme {
        axum_server::from_tcp(listener)?
            .acceptor(AcmeAcceptor::new(server_tls, responses))
            .handle(handle)
            .serve(router.into_make_service())
            .await
    } else {
        axum_server::from_tcp_rustls(listener, server_tls)?
            .handle(handle)
            .serve(router.into_make_service())
            .await
    };
    edge.shutdown();
    signal.abort();
    renewal.abort();
    if let Some(preview) = preview {
        preview.abort();
    }
    result?;
    Ok(())
}

async fn maintain_preview(
    base: String,
    credential: String,
    resolver: std::sync::Arc<EdgeCertResolver>,
    path: &'static Path,
) {
    loop {
        let result: Result<()> = async {
            let client = HttpCloudClient::new(CLOUD_ORIGIN)?;
            let managed = client.provision_preview(&credential, &base).await?;
            if managed
                && (certificate::needs_renewal(path, SystemTime::now())
                    || !saved_certificate_is_usable(path, &format!("probe.{base}")).await)
            {
                let dns = CloudDnsChallenges {
                    client: &client,
                    credential: &credential,
                    preview: true,
                };
                issue_dns_and_save(
                    &InstantAcmeIssuer::production(ACCOUNT_PATH),
                    &format!("*.{base}"),
                    path,
                    &dns,
                    &mut std::io::stdout(),
                )
                .await?;
            }
            if path.exists() {
                resolver.load_preview(path).await?;
            }
            client.confirm_preview(&credential).await
        }
        .await;
        let delay = match result {
            Ok(()) => Duration::from_secs(24 * 60 * 60),
            Err(error) => {
                tracing::warn!(%error,"preview certificate provisioning or confirmation failed");
                Duration::from_secs(300)
            }
        };
        tokio::time::sleep(delay).await;
    }
}

async fn renew_certificates(
    config: ServiceConfig,
    credential: String,
    tls: RustlsConfig,
    shared_acme: Option<pontia_edge::challenge::ChallengeResponses>,
) {
    tokio::time::sleep(FIRST_RENEWAL_CHECK).await;
    loop {
        if certificate::needs_renewal(Path::new(TLS_PATH), SystemTime::now()) {
            let result = renew_certificate(&config, &credential, &tls, shared_acme.clone()).await;
            if let Err(error) = result {
                tracing::error!(%error, "certificate renewal failed");
            } else {
                tracing::info!(hostname = %config.hostname, "certificate renewed");
            }
        }
        tokio::time::sleep(RENEWAL_CHECK_INTERVAL).await;
    }
}

async fn renew_certificate(
    config: &ServiceConfig,
    credential: &str,
    tls: &RustlsConfig,
    shared_acme: Option<pontia_edge::challenge::ChallengeResponses>,
) -> Result<()> {
    let issuer = InstantAcmeIssuer::production(ACCOUNT_PATH);
    match config.acme_challenge {
        AcmeChallenge::Http01 => {
            issue_http_and_save(
                &issuer,
                &config.hostname,
                Path::new(TLS_PATH),
                shared_acme,
                &mut std::io::stdout(),
            )
            .await?
        }
        // Edges registered under the managed zone keep renewing through Cloud DNS.
        AcmeChallenge::Dns01 => {
            let client = HttpCloudClient::new(CLOUD_ORIGIN)?;
            let dns = CloudDnsChallenges {
                client: &client,
                credential,
                preview: false,
            };
            issue_dns_and_save(
                &issuer,
                &config.hostname,
                Path::new(TLS_PATH),
                &dns,
                &mut std::io::stdout(),
            )
            .await?
        }
    }
    tls.reload_from_pem_file(TLS_PATH, TLS_PATH).await?;
    Ok(())
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

    struct FailingConfirmationClient;

    impl EdgeConfirmationClient for FailingConfirmationClient {
        async fn confirm(&self, _ticket: &str, _credential: &str, _port: u16) -> Result<String> {
            anyhow::bail!(
                "Cloud could not confirm the edge (HTTP 422 Unprocessable Entity, error: ownership_verification_failed)"
            )
        }
    }

    #[tokio::test]
    async fn confirmation_timeout_reports_progress_and_the_last_cloud_error() {
        let mut output = Vec::new();
        let error = wait_for_confirmation(
            &FailingConfirmationClient,
            "secret-ticket",
            "secret-credential",
            "edge.example.com",
            443,
            &mut output,
            1,
            Duration::ZERO,
        )
        .await
        .unwrap_err()
        .to_string();
        let output = String::from_utf8(output).unwrap();

        assert!(error.contains("HTTP 422 Unprocessable Entity"));
        assert!(error.contains("ownership_verification_failed"));
        assert!(!output.contains("secret-ticket"));
        assert!(!output.contains("secret-credential"));
        assert!(!error.contains("secret-ticket"));
        assert!(!error.contains("secret-credential"));
    }

    #[tokio::test]
    async fn failed_initialization_keeps_pending_parameters_for_retry() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("pending-init.json");
        std::fs::write(&path, "pending").unwrap();

        let result = complete_pending_initialization(&path, async {
            anyhow::bail!("initialization failed")
        })
        .await;

        assert!(result.is_err());
        assert!(path.exists());
    }

    #[tokio::test]
    async fn successful_initialization_removes_pending_parameters() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("pending-init.json");
        std::fs::write(&path, "pending").unwrap();

        complete_pending_initialization(&path, async { Ok(()) })
            .await
            .unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn retry_is_a_standalone_command() {
        let cli = Cli::try_parse_from(["pontia-edge", "retry"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Retry)));
        assert!(Cli::try_parse_from(["pontia-edge", "retry", "--ticket", "secret"]).is_err());
    }

    #[test]
    fn update_is_a_standalone_command() {
        let cli = Cli::try_parse_from(["pontia-edge", "update"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Update { automatic: false })
        ));
        assert!(Cli::try_parse_from(["pontia-edge", "update", "--ticket", "secret"]).is_err());
    }

    #[test]
    fn accepts_auto_update_management_and_the_scheduled_update_invocation() {
        for action in ["enable", "disable", "status"] {
            let cli = Cli::try_parse_from(["pontia-edge", "auto-update", action]).unwrap();
            assert!(matches!(cli.command, Some(Command::AutoUpdate { .. })));
        }
        assert!(Cli::try_parse_from(["pontia-edge", "auto-update"]).is_err());
        assert!(Cli::try_parse_from(["pontia-edge", "auto-update", "unknown"]).is_err());
        let cli = Cli::try_parse_from(["pontia-edge", "update", "--automatic"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Update { automatic: true })
        ));
    }

    #[test]
    fn deployment_wait_durations_are_readable() {
        assert_eq!(
            format_duration(Duration::from_secs(150)),
            "2 minutes 30 seconds"
        );
        assert_eq!(format_duration(Duration::from_secs(300)), "5 minutes");
    }

    #[tokio::test]
    async fn untrusted_or_expiring_certificates_are_not_reused() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("tls.pem");
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["edge.example.com".to_owned()]).unwrap();
        std::fs::write(
            &path,
            format!("{}{}", cert.pem(), signing_key.serialize_pem()),
        )
        .unwrap();

        assert!(!certificate_matches_hostname(&path, "edge.example.com"));
        assert!(!certificate_matches_hostname(&path, "other.example.com"));
        assert!(!reusable_certificate(&path, "edge.example.com").await);
        assert!(
            !reusable_certificate(&test_root.path().join("missing.pem"), "edge.example.com").await
        );
    }

    #[test]
    fn init_accepts_edge_and_preview_ports_and_always_uses_http_validation() {
        let base = [
            "pontia-edge",
            "init",
            "--edge-id",
            "0199791c-6600-7000-8000-000000000001",
            "--ticket",
            "ticket",
        ];
        for (extra, port) in [(vec![], None), (vec!["--port", "8443"], Some(8443))] {
            let cli = Cli::try_parse_from(base.into_iter().chain(extra)).unwrap();
            let Some(Command::Init(args)) = cli.command else {
                panic!("expected init")
            };
            assert_eq!(args.port, port);
            assert_eq!(args.preview_ports, None);
        }
        for value in ["off", "20000-20099", "3000-9999,20000-20099"] {
            let cli =
                Cli::try_parse_from(base.into_iter().chain(["--preview-ports", value])).unwrap();
            let Some(Command::Init(args)) = cli.command else {
                panic!("expected init")
            };
            assert_eq!(args.preview_ports.as_deref(), Some(value));
        }
        for method in ["dns-01", "http-01"] {
            assert!(
                Cli::try_parse_from(base.into_iter().chain(["--acme-challenge", method])).is_err()
            );
        }
        for port in ["0", "25", "6000", "65536"] {
            assert!(Cli::try_parse_from(base.into_iter().chain(["--port", port])).is_err());
        }
        for ranges in ["", "80", "20001-20000", "20000,"] {
            assert!(
                Cli::try_parse_from(base.into_iter().chain(["--preview-ports", ranges])).is_err()
            );
        }
    }

    #[test]
    fn command_line_initializes_without_subscriber_agreement_flag() {
        let init = Cli::try_parse_from([
            "pontia-edge",
            "init",
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
                "--edge-id",
                "0199791c-6600-7000-8000-000000000001",
                "--ticket",
                "pet_v1_example",
                "--cloud-origin",
                "https://pontia.example",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "pontia-edge",
                "init",
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
