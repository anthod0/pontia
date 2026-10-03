use std::{path::Path, process::Command, time::Duration};

use anyhow::{Context, Result};
use pontia_edge::{
    config::{CONFIG_PATH, ServiceConfig},
    credential::ensure_root,
    systemd::Systemd,
};

const EXECUTABLE: &str = "/usr/local/bin/pontia-edge";

pub async fn run() -> Result<()> {
    ensure_root(rustix::process::geteuid().as_raw())?;
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    anyhow::ensure!(
        executable == Path::new(EXECUTABLE),
        "update requires the installation at {EXECUTABLE}"
    );
    anyhow::ensure!(
        Path::new("/run/systemd/system").is_dir(),
        "edge update requires a running systemd manager"
    );
    let update = pontia_update::prepare("pontia-edge", &["pontia-edge"])
        .await
        .map_err(anyhow::Error::msg)?;
    // Inspect service state after downloading, immediately before replacement.
    let systemd = Systemd::default();
    let running = systemd.update_is_active()?;
    let config = if running {
        let output = Command::new("systemctl")
            .args([
                "show",
                "--property=MainPID",
                "--value",
                "pontia-edge.service",
            ])
            .output()
            .context("failed to read edge service PID")?;
        anyhow::ensure!(output.status.success(), "failed to read edge service PID");
        let pid: u32 = std::str::from_utf8(&output.stdout)?.trim().parse()?;
        anyhow::ensure!(
            std::fs::read_link(format!("/proc/{pid}/exe"))? == executable,
            "the running edge service belongs to another installation"
        );
        Some(ServiceConfig::read(Path::new(CONFIG_PATH))?)
    } else {
        ensure_no_other_edge(&executable)?;
        None
    };
    let client = config.as_ref().map(health_client).transpose()?;
    tokio::task::block_in_place(|| {
        update
            .install(|| {
                let (Some(config), Some(client)) = (&config, &client) else {
                    return Ok(());
                };
                systemd.restart().map_err(|error| error.to_string())?;
                tokio::runtime::Handle::current()
                    .block_on(wait_for_health(client, config))
                    .map_err(|error| error.to_string())?;
                if !systemd
                    .update_is_active()
                    .map_err(|error| error.to_string())?
                {
                    return Err("edge service is not active after restart".into());
                }
                Ok(())
            })
            .map_err(anyhow::Error::msg)
    })
}

fn ensure_no_other_edge(executable: &Path) -> Result<()> {
    for process in std::fs::read_dir("/proc")? {
        let process = process?;
        let Ok(pid) = process.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        if let Ok(path) = std::fs::read_link(process.path().join("exe")) {
            anyhow::ensure!(
                path != executable
                    && path != Path::new(&format!("{} (deleted)", executable.display())),
                "an unmanaged edge process is running; stop it before updating"
            );
        }
    }
    Ok(())
}

fn health_client(config: &ServiceConfig) -> Result<reqwest::Client> {
    // Probe this machine, not public DNS or a proxy, while verifying the TLS hostname.
    Ok(reqwest::Client::builder()
        .no_proxy()
        .resolve(&config.hostname, ([127, 0, 0, 1], config.port).into())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()?)
}

async fn wait_for_health(client: &reqwest::Client, config: &ServiceConfig) -> Result<()> {
    let url = format!("https://{}:{}/healthz", config.hostname, config.port);
    for _ in 0..20 {
        if let Ok(response) = client.get(&url).send().await
            && response.status().is_success()
            && response.text().await.is_ok_and(|body| body == "ok")
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("edge did not become healthy at {url} after restart")
}
