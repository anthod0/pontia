use std::path::Path;

use anyhow::{Context, Result};
use pontia_edge::{credential::ensure_root, systemd::Systemd};

const EXECUTABLE: &str = "/usr/local/bin/pontia-edge";

pub fn ensure_managed_installation() -> Result<std::path::PathBuf> {
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
    Ok(executable)
}

pub async fn run(automatic: bool) -> Result<()> {
    let executable = ensure_managed_installation()?;
    let systemd = Systemd::default();
    if automatic && !systemd.update_is_active().await? {
        println!("Edge service is stopped; skipping automatic update.");
        return Ok(());
    }
    let Some(update) = pontia_update::prepare(
        "pontia-edge",
        &["pontia-edge"],
        Some(pontia_version::version()),
    )
    .await
    .map_err(anyhow::Error::msg)?
    else {
        return Ok(());
    };
    // Inspect service state after downloading, immediately before replacement.
    let running = systemd.update_is_active().await?;
    if automatic && !running {
        println!("Edge service stopped during download; skipping automatic update.");
        return Ok(());
    }
    if running {
        let running_process = systemd
            .update_process()
            .await?
            .context("edge service has no running process")?;
        anyhow::ensure!(
            running_process.executable == executable,
            "the running edge service belongs to another installation"
        );
    } else {
        ensure_no_other_edge(&executable)?;
    }
    tokio::task::block_in_place(|| {
        update
            .install(|| {
                if !running {
                    return Ok(());
                }
                let runtime = tokio::runtime::Handle::current();
                runtime
                    .block_on(systemd.restart_for_update(&executable))
                    .map_err(|error| error.to_string())
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
