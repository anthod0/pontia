use std::{path::Path, process::Command};

use anyhow::{Context, Result};

use crate::files::atomic_write;

pub const UNIT_PATH: &str = "/etc/systemd/system/pontia-edge.service";
pub const UNIT: &str = r#"[Unit]
Description=Pontia Edge
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/pontia-edge
Restart=on-failure
RestartSec=5s
User=root
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=/etc/pontia/edge

[Install]
WantedBy=multi-user.target
"#;

pub trait CommandRunner {
    fn run(&self, arguments: &[&str]) -> Result<bool>;
}

pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, arguments: &[&str]) -> Result<bool> {
        Ok(Command::new("systemctl")
            .args(arguments)
            .status()
            .with_context(|| format!("failed to run systemctl {}", arguments.join(" ")))?
            .success())
    }
}

pub struct Systemd<R = ProcessCommandRunner> {
    runner: R,
}

impl Default for Systemd {
    fn default() -> Self {
        Self {
            runner: ProcessCommandRunner,
        }
    }
}

impl<R: CommandRunner> Systemd<R> {
    pub fn is_active(&self) -> bool {
        self.runner
            .run(&["is-active", "--quiet", "pontia-edge.service"])
            .unwrap_or(false)
    }

    pub fn update_is_active(&self) -> Result<bool> {
        self.runner
            .run(&["is-active", "--quiet", "pontia-edge.service"])
    }

    pub fn restart(&self) -> Result<()> {
        self.run_required(&["restart", "pontia-edge.service"])
    }

    pub fn install_and_start(&self, unit_path: &Path) -> Result<()> {
        atomic_write(unit_path, UNIT.as_bytes(), 0o644)?;
        self.run_required(&["daemon-reload"])?;
        self.run_required(&["enable", "--now", "pontia-edge.service"])
    }

    fn run_required(&self, arguments: &[&str]) -> Result<()> {
        anyhow::ensure!(
            self.runner.run(arguments)?,
            "systemctl {} failed",
            arguments.join(" ")
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, arguments: &[&str]) -> Result<bool> {
            self.calls
                .lock()
                .unwrap()
                .push(arguments.iter().map(|value| (*value).to_owned()).collect());
            Ok(true)
        }
    }

    #[test]
    fn update_checks_status_and_restarts_without_reinstalling_the_unit() {
        let systemd = Systemd {
            runner: FakeRunner {
                calls: Mutex::new(Vec::new()),
            },
        };
        assert!(systemd.update_is_active().unwrap());
        systemd.restart().unwrap();
        assert_eq!(
            *systemd.runner.calls.lock().unwrap(),
            [
                vec![
                    "is-active".to_owned(),
                    "--quiet".to_owned(),
                    "pontia-edge.service".to_owned()
                ],
                vec!["restart".to_owned(), "pontia-edge.service".to_owned()],
            ]
        );
    }

    #[test]
    fn restart_failure_is_not_reported_as_success() {
        struct FailingRunner;
        impl CommandRunner for FailingRunner {
            fn run(&self, _arguments: &[&str]) -> Result<bool> {
                Ok(false)
            }
        }
        let systemd = Systemd {
            runner: FailingRunner,
        };
        assert!(!systemd.update_is_active().unwrap());
        assert!(systemd.restart().is_err());
    }

    #[test]
    fn installs_the_embedded_unit_before_reloading_and_starting_systemd() {
        let test_root = tempfile::tempdir().unwrap();
        let unit_path = test_root.path().join("pontia-edge.service");
        let systemd = Systemd {
            runner: FakeRunner {
                calls: Mutex::new(Vec::new()),
            },
        };

        systemd.install_and_start(&unit_path).unwrap();

        assert_eq!(std::fs::read_to_string(unit_path).unwrap(), UNIT);
        assert_eq!(
            *systemd.runner.calls.lock().unwrap(),
            [
                vec!["daemon-reload".to_owned()],
                vec![
                    "enable".to_owned(),
                    "--now".to_owned(),
                    "pontia-edge.service".to_owned()
                ],
            ]
        );
    }
}
