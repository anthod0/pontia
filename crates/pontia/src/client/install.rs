use super::ClientApi;
use clap::Args;
use pontia_agent_control::client_contract::{
    DetectionContext,
    install::{ClientInstaller, InstallMethod, InstallProvenance, InstallSpec},
};
use pontia_config::AppConfig;
use pontia_runtime::client_operations::{self as operations, Phase, Status};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::Path,
};

#[derive(Debug, Args)]
pub(super) struct Options {
    #[arg(required_unless_present = "list", conflicts_with = "list")]
    client: Option<String>,
    /// Show installation methods and prerequisites
    #[arg(long, conflicts_with_all=["method","no_setup","no_enable","yes"])]
    list: bool,
    #[arg(long)]
    method: Option<String>,
    #[arg(long)]
    no_setup: bool,
    #[arg(long)]
    no_enable: bool,
    /// Skip the installation confirmation
    #[arg(long)]
    yes: bool,
}

fn method<'a>(spec: &'a InstallSpec, id: Option<&str>) -> Result<&'a InstallMethod, String> {
    spec.methods
        .iter()
        .find(|m| id.map_or(m.recommended, |id| m.id == id))
        .ok_or_else(|| {
            format!(
                "No matching installation method: {}",
                id.unwrap_or("recommended")
            )
        })
}

fn allowed(spec: &InstallSpec, method: &InstallMethod) -> io::Result<()> {
    if spec.provenance != InstallProvenance::NotInstalled {
        return Err(io::Error::other("Client is already installed"));
    }
    if !method.ready() {
        let hints = method
            .prerequisites
            .iter()
            .filter(|p| p.required && !p.satisfied)
            .map(|p| p.missing_hint.as_deref().unwrap_or(&p.label))
            .collect::<Vec<_>>();
        return Err(io::Error::other(if hints.is_empty() {
            "Installation method unavailable on this platform".into()
        } else {
            hints.join("\n")
        }));
    }
    Ok(())
}

fn display_log(home: &Path, id: uuid::Uuid, offset: &mut u64) -> io::Result<()> {
    loop {
        let bytes = operations::read_log(home, id, *offset)?;
        if bytes.is_empty() {
            return Ok(());
        }
        io::stderr().write_all(&bytes)?;
        *offset += bytes.len() as u64;
    }
}

pub(super) async fn run(options: Options, config: AppConfig) -> Result<Value, String> {
    let registry = pontia_clients::registration();
    let vars = std::env::vars().collect();
    let context = DetectionContext {
        vars: &vars,
        pontia_home: &config.pontia_home,
    };
    let mut entries = Vec::new();
    let mut selected = None;
    for registration in registry.registrations() {
        let Some(configuration) = &registration.configuration else {
            continue;
        };
        for key in configuration.keys() {
            let Some(installer) = registry.installer(registration.spec.client_type, &key) else {
                continue;
            };
            let effective = configuration.merged(&key);
            let (_, short_id) = configuration.presentation(&key, &effective);
            if options.list {
                entries.push(json!({"client":short_id,"detection":configuration.detect(&effective,&context),"install":installer.inspect(&context)}));
            } else if options.client.as_ref() == Some(&short_id)
                && selected.replace(installer).is_some()
            {
                return Err(format!("Ambiguous install selector: {short_id}"));
            }
        }
    }
    if options.list {
        return Ok(json!(entries));
    }
    let installer = selected.ok_or_else(|| {
        format!(
            "No installer registered for {}",
            options.client.as_deref().unwrap_or_default()
        )
    })?;
    let spec = installer.inspect(&context);
    let chosen = method(&spec, options.method.as_deref())?;
    let client = installer.client_type();
    let key = installer.client_key();
    let setup = !options.no_setup && registry.integration(client, key).is_some();
    let enable = !options.no_enable;
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&json!({"method":chosen,"setup":setup,"enable":enable}))
            .unwrap()
    );
    if !options.yes {
        eprint!("Install this client? [y/N] ");
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err("Installation declined".into());
        }
    }
    let method_id = chosen.id.clone();
    let home = config.pontia_home.clone();
    let handle = tokio::runtime::Handle::current();
    let (started, id) = tokio::sync::oneshot::channel();
    let task = tokio::task::spawn_blocking(move || {
        let context = DetectionContext {
            vars: &vars,
            pontia_home: &config.pontia_home,
        };
        operations::run(&config.pontia_home, client, key, &method_id, |op| {
            let _ = started.send(op.id());
            install(installer.as_ref(), &context, &method_id, op)?;
            if setup {
                op.advance(Phase::SettingUp)?;
                let mut worker = std::process::Command::new(std::env::current_exe()?);
                worker.args(["install-setup", client]).env(
                    "PATH",
                    std::env::join_paths(context.search_path()).map_err(io::Error::other)?,
                );
                op.log("Configuring the client integration")?;
                op.run_setup(&mut worker)?;
            }
            op.checkpoint()?;
            let config = if setup {
                crate::local_config::load(&vars).map_err(io::Error::other)?
            } else {
                config.clone()
            };
            handle.block_on(refresh(&config, client, key, enable, op))
        })
    });
    let state = monitor(task, id, &home).await?;
    let headless = std::env::var_os("SSH_CONNECTION").is_some()
        || (cfg!(target_os = "linux")
            && std::env::var_os("DISPLAY").is_none()
            && std::env::var_os("WAYLAND_DISPLAY").is_none());
    let login = if headless {
        spec.auth
            .headless_login_command
            .as_ref()
            .or(spec.auth.login_command.as_ref())
    } else {
        spec.auth.login_command.as_ref()
    };
    Ok(json!({"operation":state,"auth":spec.auth,"login_command":login}))
}

async fn refresh(
    config: &AppConfig,
    client: &str,
    key: &str,
    enable: bool,
    op: &mut operations::InstallOperation,
) -> io::Result<()> {
    let addr = crate::lifecycle::local_health_addr(config.bind_addr);
    match tokio::net::TcpStream::connect(addr).await {
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            return op.log("Pontia service is offline. Run pontia up to refresh client status.");
        }
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    let api = ClientApi::new(config).map_err(io::Error::other)?;
    if enable {
        api.patch(client, key, json!({"enabled":true}))
            .await
            .map_err(io::Error::other)?;
    } else {
        api.list().await.map_err(io::Error::other)?;
    }
    Ok(())
}

async fn monitor(
    mut task: tokio::task::JoinHandle<io::Result<operations::InstallState>>,
    id: tokio::sync::oneshot::Receiver<uuid::Uuid>,
    home: &Path,
) -> Result<operations::InstallState, String> {
    let mut offset = 0;
    let id = id.await.ok();
    let result = if let Some(id) = id {
        let signal = tokio::signal::ctrl_c();
        tokio::pin!(signal);
        let mut cancelled = false;
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            tokio::select! {
                result = &mut task => break result,
                Ok(()) = &mut signal, if !cancelled => {
                    cancelled = true;
                    if let Err(error) = operations::cancel(home, id) { eprintln!("{error}"); }
                },
                _ = timer.tick() => display_log(home, id, &mut offset).map_err(|e| e.to_string())?,
            }
        }
    } else {
        task.await
    };
    let state = result.map_err(|e| e.to_string())?.map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock
            && let Ok(Some(active)) = operations::latest(home)
        {
            return format!("Install operation {} is in progress", active.id);
        }
        e.to_string()
    })?;
    display_log(home, state.id, &mut offset).map_err(|e| e.to_string())?;
    if state.status != Status::Succeeded {
        return Err(state
            .error
            .unwrap_or_else(|| "Installation cancelled".into()));
    }
    Ok(state)
}

fn install(
    installer: &dyn ClientInstaller,
    context: &DetectionContext,
    method_id: &str,
    op: &mut operations::InstallOperation,
) -> io::Result<()> {
    let fresh = installer.inspect(context);
    let chosen = method(&fresh, Some(method_id)).map_err(io::Error::other)?;
    allowed(&fresh, chosen)?;
    op.log(&format!(
        "Installing {}/{} using {method_id}",
        installer.client_type(),
        installer.client_key()
    ))?;
    installer.install(method_id, op)?;
    if op.phase() != Phase::Verifying {
        return Err(io::Error::other("Installer did not reach verification"));
    }
    op.checkpoint()
}

#[cfg(test)]
mod tests;
