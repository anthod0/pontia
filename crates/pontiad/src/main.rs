mod background;
mod e2e_identity;
mod initialization;
mod shutdown;
mod uploads;
use background::BackgroundTasks;
use pontia_agent_control::client_contract::ClientServicePhase;
use pontia_core::error::Result;
use pontia_http as http;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--stdio-relay") => return pontia_clients::run_stdio_relay().await,
        Some("shutdown") => {
            if let Err(error) = shutdown::run().await {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return Ok(());
        }
        _ => {}
    }
    let config = pontia_config::AppConfig::from_env()?;
    init_tracing();
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let app_state = initialization::initialize(&config).await?;
    let shutdown = app_state.control().shutdown();
    let inbox = app_state.control().inbox_commands();
    let mut background = BackgroundTasks::default();
    let server = serve(config, app_state, listener, &mut background).await;
    shutdown.notify();
    let tasks = background.join().await;
    inbox.stop_scheduling().await;
    server.and(tasks)
}

async fn serve(
    config: pontia_config::AppConfig,
    app_state: pontia_application::AppState,
    listener: tokio::net::TcpListener,
    background: &mut BackgroundTasks,
) -> Result<()> {
    let bound_addr = listener.local_addr()?;
    let transports = app_state
        .control()
        .clients()
        .start_services(app_state.control().clone(), ClientServicePhase::Transport)
        .await?;
    background.spawn_service(transports.join());
    let cache_control = app_state.control().clone();
    background.spawn(async move { cache_control.run_timeline_cache_cleanup().await });
    let inbox = app_state.control().inbox_commands();
    let runtime_observer = app_state.control().runtime_observer();
    background.spawn(runtime_observer.run(app_state.control().shutdown().subscribe()));
    let observers = app_state
        .control()
        .clients()
        .start_services(app_state.control().clone(), ClientServicePhase::Observation)
        .await?;
    background.spawn_service(observers.join());
    let workflows =
        pontia_workflow::WorkflowState::new(app_state.db(), app_state.control()).await?;
    let runtime = pontia_workflow::WorkflowRuntime::new(workflows.clone(), app_state.control());
    background.spawn_service(runtime.run(app_state.control().shutdown().subscribe()));
    let dashboard =
        http::dashboard::resolve_dashboard(&config.dashboard, &config.pontia_home).await;
    let agent_clients = pontia_agent_control::agent_clients::AgentClientService::new(
        app_state.db(),
        app_state.control().clients(),
    );
    let bots = pontia_bot::conversation::ConversationService::new(
        app_state.control().clone(),
        std::env::home_dir(),
        app_state.workspace_browser().roots,
    )
    .with_tool_address(local_addr(bound_addr))
    .with_generation_limits(pontia_bot::conversation::GenerationLimits::new(
        config.bot.context_token_limit.get(),
        config.bot.context_window_percent,
    )?);
    let bot = http::BotState::new(app_state.clone(), bots);

    info!(addr = %bound_addr, "starting pontia control plane");
    info!(url = %dashboard_url(bound_addr), "dashboard available");

    let shutdown = app_state.control().shutdown();
    background.spawn(bot.bots().clone().run_generations(shutdown.subscribe()));
    background.spawn(bot.system_notices().run(shutdown.subscribe()));
    let helper_events = bot.helpers().register_events().await?;
    let helper_shutdown = shutdown.subscribe();
    background.spawn_service(async move { helper_events.run(helper_shutdown).await });
    background.spawn(
        bot.helpers()
            .notification_delivery()
            .run(shutdown.subscribe()),
    );
    background.spawn(bot.dreaming().clone().run(shutdown.subscribe()));
    let cleanup_shutdown = shutdown.clone();
    let external_api = http::core_router(app_state.clone())
        .merge(http::bot_router(bot))
        .merge(http::workflow_router(app_state.clone(), workflows));
    let http_entrypoints =
        http::HttpEntrypoints::new(http::HttpState::new(app_state, dashboard), external_api);
    let remote = config
        .remote
        .as_ref()
        .and_then(|remote| remote.device_id.as_deref())
        .map(|device_id| {
            let device_id = uuid::Uuid::parse_str(device_id).map_err(|_| {
                pontia_core::error::Error::InvalidConfig {
                    key: "remote.device_id",
                    message: "must be a UUIDv7".to_string(),
                }
            })?;
            if device_id.get_version_num() != 7 {
                return Err(pontia_core::error::Error::InvalidConfig {
                    key: "remote.device_id",
                    message: "must be a UUIDv7".to_string(),
                });
            }
            let (sessions, cloud_key) = e2e_identity::load(&config.pontia_home, device_id)
                .map_err(|message| pontia_core::error::Error::InvalidConfig {
                    key: "remote.device_id",
                    message,
                })?;
            let ingress = http_entrypoints.e2e_tunnel(sessions);
            let maintenance = ingress.clone();
            let mut maintenance_shutdown = cleanup_shutdown.subscribe();
            background.spawn(async move {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(60)) => maintenance.reap(),
                        _ = maintenance_shutdown.wait_for(|stop| *stop) => break,
                    }
                }
            });
            let preview = pontiad::LocalPreviewProxy::new(
                bound_addr.port(),
                config.preview,
                device_id,
                cloud_key,
                cleanup_shutdown.subscribe(),
            );
            let handler = pontia_tunnel::DeviceRequestHandler::new(move |request| {
                let ingress = ingress.clone();
                let preview = preview.clone();
                async move {
                    preview
                        .dispatch(request, async |req| ingress.handle(req).await)
                        .await
                }
            });
            pontia_tunnel::RemoteClient::new(
                &config.auth_origin,
                device_id,
                &config.pontia_home,
                handler,
            )
            .map_err(|error| pontia_core::error::Error::InvalidConfig {
                key: "PONTIA_AUTH_ORIGIN",
                message: error.to_string(),
            })
        })
        .transpose()?;
    if let Some(remote) = remote {
        background.spawn(remote.run(cleanup_shutdown.subscribe()));
    }
    inbox.resume_pending().await?;
    background.spawn(uploads::run(
        config.pontia_home.clone(),
        config.uploads,
        cleanup_shutdown.subscribe(),
    ));
    let mut probe_shutdown = cleanup_shutdown.subscribe();
    background.spawn(async move {
        tokio::select! {
            _ = probe_shutdown.wait_for(|stop| *stop) => {}
            _ = agent_clients.probe_enabled(config.pontia_home, true) => {}
        }
    });
    http::serve_with_shutdown_timeout(
        listener,
        http_entrypoints.local_http(),
        async move {
            shutdown_signal().await;
            shutdown.notify();
        },
        Duration::from_secs(5),
    )
    .await
}

fn local_addr(addr: SocketAddr) -> SocketAddr {
    let host = if addr.ip().is_unspecified() {
        match addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    } else {
        addr.ip()
    };

    SocketAddr::new(host, addr.port())
}

fn dashboard_url(addr: SocketAddr) -> String {
    format!("http://{}/dashboard", local_addr(addr))
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
