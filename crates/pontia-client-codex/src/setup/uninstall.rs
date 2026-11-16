use super::{codex_service_path, shim::ShimUninstall};
use pontia_agent_control::client_contract::PreparedClientUninstall;
use pontia_runtime::local_service::{CommandRunner, remove_file_if_present};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

struct Uninstall {
    shim: ShimUninstall,
    service: PathBuf,
    #[cfg(target_os = "macos")]
    domain: String,
}

pub(super) fn prepare(
    vars: &HashMap<String, String>,
    user_home: &Path,
    _runner: &dyn CommandRunner,
) -> Result<Option<Box<dyn PreparedClientUninstall>>, String> {
    let root = vars
        .get("PONTIA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".pontia"));
    let shim = ShimUninstall::inspect(&root)?;
    let service = codex_service_path(user_home);
    if shim.summary().is_empty() && !service.try_exists().map_err(|error| error.to_string())? {
        return Ok(None);
    }
    #[cfg(target_os = "macos")]
    let domain = {
        let output = _runner.run("id", &["-u".into()])?;
        let uid = output
            .stdout
            .trim()
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        if output.code != 0 {
            return Err("could not determine the current user ID".into());
        }
        format!("gui/{uid}")
    };
    Ok(Some(Box::new(Uninstall {
        shim,
        service,
        #[cfg(target_os = "macos")]
        domain,
    })))
}

impl PreparedClientUninstall for Uninstall {
    fn summary(&self) -> Vec<String> {
        let mut summary = self.shim.summary();
        if self.service.exists() {
            summary.push(format!(
                "Remove Codex autostart: {}",
                self.service.display()
            ));
        }
        summary
    }

    fn uninstall(&self, runner: &dyn CommandRunner) -> Result<(), String> {
        if self
            .service
            .try_exists()
            .map_err(|error| error.to_string())?
        {
            #[cfg(target_os = "linux")]
            require(
                runner,
                "systemctl",
                &[
                    "--user",
                    "disable",
                    "--now",
                    super::CODEX_SYSTEMD_SERVICE_NAME,
                ],
            )?;
            #[cfg(target_os = "macos")]
            {
                let target = format!("{}/{}", self.domain, super::CODEX_LAUNCHD_LABEL);
                let status = runner.run("launchctl", &["print".into(), target.clone()])?;
                if status.code == 0 {
                    require(runner, "launchctl", &["bootout", &target])?;
                } else if !status.stderr.contains("Could not find service") {
                    return Err(format!(
                        "could not inspect Codex autostart: {}",
                        status.stderr.trim()
                    ));
                }
                require(runner, "launchctl", &["disable", &target])?;
            }
            remove_file_if_present(&self.service)?;
            #[cfg(target_os = "linux")]
            require(runner, "systemctl", &["--user", "daemon-reload"])?;
        }
        self.shim.uninstall()
    }
}

fn require(runner: &dyn CommandRunner, program: &str, args: &[&str]) -> Result<(), String> {
    let output = runner.run(
        program,
        &args.iter().map(|arg| (*arg).into()).collect::<Vec<_>>(),
    )?;
    if output.code == 0 {
        Ok(())
    } else {
        Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            output.stderr.trim()
        ))
    }
}
