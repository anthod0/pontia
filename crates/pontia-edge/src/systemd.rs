use std::{future::Future, path::Path, time::Duration};

use tokio::process::Command;

use anyhow::{Context, Result};

use crate::files::atomic_write;

pub const UNIT_PATH: &str = "/etc/systemd/system/pontia-edge.service";
pub const UNIT_DIRECTORY: &str = "/etc/systemd/system";
const UPDATE_SERVICE: &str = "pontia-edge-update.service";
const UPDATE_TIMER: &str = "pontia-edge-update.timer";
const UPDATE_SERVICE_CONTENTS: &str = include_str!("../systemd/pontia-edge-update.service");
const UPDATE_TIMER_CONTENTS: &str = include_str!("../systemd/pontia-edge-update.timer");
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

pub struct CommandOutput {
    success: bool,
    stdout: String,
}

impl CommandOutput {
    #[cfg(test)]
    fn success(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            stdout: stdout.into(),
        }
    }
}

pub trait CommandRunner {
    fn run(&self, arguments: &[&str]) -> impl Future<Output = Result<CommandOutput>> + Send;
}

pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    async fn run(&self, arguments: &[&str]) -> Result<CommandOutput> {
        let mut command = Command::new("systemctl");
        command.args(arguments).kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .with_context(|| {
                format!(
                    "systemctl {} timed out after 30 seconds",
                    arguments.join(" ")
                )
            })?
            .with_context(|| format!("failed to run systemctl {}", arguments.join(" ")))?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8(output.stdout).context("systemctl output is not UTF-8")?,
        })
    }
}

pub struct Systemd<R = ProcessCommandRunner> {
    runner: R,
}

pub struct ServiceProcess {
    pub pid: u32,
    pub executable: std::path::PathBuf,
}

impl Default for Systemd {
    fn default() -> Self {
        Self {
            runner: ProcessCommandRunner,
        }
    }
}

impl<R: CommandRunner> Systemd<R> {
    pub async fn is_active(&self) -> bool {
        self.runner
            .run(&["is-active", "--quiet", "pontia-edge.service"])
            .await
            .is_ok_and(|output| output.success)
    }

    pub async fn update_is_active(&self) -> Result<bool> {
        Ok(self
            .runner
            .run(&["is-active", "--quiet", "pontia-edge.service"])
            .await?
            .success)
    }

    pub async fn update_process(&self) -> Result<Option<ServiceProcess>> {
        let arguments = [
            "show",
            "pontia-edge.service",
            "--property=ActiveState",
            "--property=MainPID",
        ];
        let output = self.runner.run(&arguments).await?;
        anyhow::ensure!(output.success, "systemctl {} failed", arguments.join(" "));
        let active = output
            .stdout
            .lines()
            .find_map(|line| line.strip_prefix("ActiveState="))
            == Some("active");
        if !active {
            return Ok(None);
        }
        let pid = output
            .stdout
            .lines()
            .find_map(|line| line.strip_prefix("MainPID="))
            .context("edge service status did not include MainPID")?
            .parse::<u32>()
            .context("edge service has an invalid MainPID")?;
        anyhow::ensure!(pid != 0, "edge service is active but has no main process");
        let executable = std::fs::read_link(format!("/proc/{pid}/exe"))
            .context("cannot resolve the running edge process")?;
        Ok(Some(ServiceProcess { pid, executable }))
    }

    pub async fn restart_for_update(&self, expected_executable: &Path) -> Result<()> {
        self.restart().await?;
        self.wait_for_stable_update_process(expected_executable)
            .await
    }

    pub async fn wait_for_stable_update_process(&self, expected_executable: &Path) -> Result<()> {
        const VERIFICATION_CHECKS: usize = 10;
        const CHECK_INTERVAL: Duration = Duration::from_secs(1);

        let mut process = self
            .update_process()
            .await?
            .context("edge service is not active after restart")?;
        // Type=simple reports the service active before its process has exec'd.
        // Give that same process time to enter the installed executable.
        for _ in 0..VERIFICATION_CHECKS {
            if process.executable == expected_executable {
                break;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
            let current = self
                .update_process()
                .await?
                .context("edge service stopped before executing the updated binary")?;
            anyhow::ensure!(
                current.pid == process.pid,
                "edge service restarted before executing the updated binary"
            );
            process = current;
        }
        anyhow::ensure!(
            process.executable == expected_executable,
            "the restarted edge service is running {}, expected {}",
            process.executable.display(),
            expected_executable.display()
        );
        for _ in 0..VERIFICATION_CHECKS {
            tokio::time::sleep(CHECK_INTERVAL).await;
            let current = self
                .update_process()
                .await?
                .context("edge service stopped during startup verification")?;
            anyhow::ensure!(
                current.pid == process.pid,
                "edge service restarted during startup verification"
            );
            anyhow::ensure!(
                current.executable == expected_executable,
                "the edge service stopped running the updated executable"
            );
        }
        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        self.run_required(&["restart", "pontia-edge.service"]).await
    }

    pub async fn install_and_start(&self, unit_path: &Path) -> Result<()> {
        atomic_write(unit_path, UNIT.as_bytes(), 0o644)?;
        self.run_required(&["daemon-reload"]).await?;
        self.run_required(&["enable", "--now", "pontia-edge.service"])
            .await
    }

    pub async fn enable_auto_update(&self, unit_directory: &Path) -> Result<()> {
        atomic_write(
            &unit_directory.join(UPDATE_SERVICE),
            UPDATE_SERVICE_CONTENTS.as_bytes(),
            0o644,
        )?;
        atomic_write(
            &unit_directory.join(UPDATE_TIMER),
            UPDATE_TIMER_CONTENTS.as_bytes(),
            0o644,
        )?;
        self.run_required(&["daemon-reload"]).await?;
        self.run_required(&["enable", "--now", UPDATE_TIMER]).await
    }

    pub async fn disable_auto_update(&self) -> Result<()> {
        // Do not stop an update already in progress: it may be rolling back.
        self.run_required(&["disable", "--now", UPDATE_TIMER]).await
    }

    pub async fn show_auto_update(&self) -> Result<()> {
        for arguments in [
            [
                "show",
                UPDATE_TIMER,
                "--property=LoadState,UnitFileState,ActiveState,NextElapseUSecRealtime,LastTriggerUSec",
            ],
            [
                "show",
                UPDATE_SERVICE,
                "--property=ActiveState,Result,ExecMainStatus,ExecMainExitTimestamp",
            ],
        ] {
            let output = self.runner.run(&arguments).await?;
            anyhow::ensure!(output.success, "systemctl {} failed", arguments.join(" "));
            print!("{}", output.stdout);
        }
        Ok(())
    }

    async fn run_required(&self, arguments: &[&str]) -> Result<()> {
        anyhow::ensure!(
            self.runner.run(arguments).await?.success,
            "systemctl {} failed",
            arguments.join(" ")
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, process::Command as StdCommand, sync::Mutex};

    use super::*;

    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl CommandRunner for FakeRunner {
        async fn run(&self, arguments: &[&str]) -> Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(arguments.iter().map(|value| (*value).to_owned()).collect());
            Ok(CommandOutput::success(""))
        }
    }

    struct ScriptedRunner {
        outputs: Mutex<VecDeque<CommandOutput>>,
    }

    impl CommandRunner for ScriptedRunner {
        async fn run(&self, arguments: &[&str]) -> Result<CommandOutput> {
            assert!(matches!(
                arguments,
                ["restart", "pontia-edge.service"]
                    | [
                        "show",
                        "pontia-edge.service",
                        "--property=ActiveState",
                        "--property=MainPID"
                    ]
            ));
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .context("unexpected systemctl call")
        }
    }

    fn active_process(pid: u32) -> CommandOutput {
        CommandOutput::success(format!("ActiveState=active\nMainPID={pid}\n"))
    }

    struct TimerRunner {
        directory: std::path::PathBuf,
        state: Mutex<TimerState>,
    }

    #[derive(Default)]
    struct TimerState {
        loaded: bool,
        enabled: bool,
        active: bool,
        update_running: bool,
    }

    impl CommandRunner for TimerRunner {
        async fn run(&self, arguments: &[&str]) -> Result<CommandOutput> {
            let mut state = self.state.lock().unwrap();
            let success = match arguments {
                ["daemon-reload"] => {
                    state.loaded = self.directory.join(UPDATE_SERVICE).is_file()
                        && self.directory.join(UPDATE_TIMER).is_file();
                    state.loaded
                }
                ["enable", "--now", UPDATE_TIMER] if state.loaded => {
                    state.enabled = true;
                    state.active = true;
                    true
                }
                ["disable", "--now", UPDATE_TIMER] => {
                    state.enabled = false;
                    state.active = false;
                    true
                }
                ["stop", UPDATE_SERVICE] => {
                    state.update_running = false;
                    true
                }
                _ => anyhow::bail!("unexpected systemctl arguments: {arguments:?}"),
            };
            Ok(CommandOutput {
                success,
                stdout: String::new(),
            })
        }
    }

    #[tokio::test]
    async fn enabling_installs_and_activates_timer_and_disabling_preserves_in_flight_update() {
        let root = tempfile::tempdir().unwrap();
        let systemd = Systemd {
            runner: TimerRunner {
                directory: root.path().to_path_buf(),
                state: Mutex::new(TimerState {
                    update_running: true,
                    ..Default::default()
                }),
            },
        };
        for _ in 0..2 {
            systemd.enable_auto_update(root.path()).await.unwrap();
            let state = systemd.runner.state.lock().unwrap();
            assert!(state.enabled && state.active);
        }
        let service = std::fs::read_to_string(root.path().join(UPDATE_SERVICE)).unwrap();
        let timer = std::fs::read_to_string(root.path().join(UPDATE_TIMER)).unwrap();
        assert!(service.contains("ExecStart=/usr/local/bin/pontia-edge update --automatic"));
        assert!(service.contains("ReadWritePaths=/usr/local/bin"));
        assert!(timer.contains("OnCalendar=hourly"));
        assert!(timer.contains("RandomizedDelaySec=10m"));
        assert!(timer.contains("Persistent=true"));
        systemd.disable_auto_update().await.unwrap();
        let state = systemd.runner.state.lock().unwrap();
        assert!(!state.enabled && !state.active);
        assert!(state.update_running);
    }

    #[tokio::test]
    async fn cannot_enable_updates_when_unit_installation_fails() {
        let root = tempfile::tempdir().unwrap();
        let not_directory = root.path().join("file");
        std::fs::write(&not_directory, b"occupied").unwrap();
        let systemd = Systemd {
            runner: TimerRunner {
                directory: not_directory.clone(),
                state: Mutex::new(TimerState::default()),
            },
        };
        assert!(systemd.enable_auto_update(&not_directory).await.is_err());
        let state = systemd.runner.state.lock().unwrap();
        assert!(!state.enabled && !state.active);
    }

    #[tokio::test]
    async fn update_checks_status_and_restarts_without_reinstalling_the_unit() {
        let systemd = Systemd {
            runner: FakeRunner {
                calls: Mutex::new(Vec::new()),
            },
        };
        assert!(systemd.update_is_active().await.unwrap());
        systemd.restart().await.unwrap();
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

    #[tokio::test(start_paused = true)]
    async fn update_restart_succeeds_using_only_stable_local_process_evidence() {
        let mut process = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let expected_executable =
            std::fs::read_link(format!("/proc/{}/exe", process.id())).unwrap();
        let mut outputs = VecDeque::from([CommandOutput::success("")]);
        outputs.extend(std::iter::repeat_with(|| active_process(process.id())).take(11));
        let systemd = Systemd {
            runner: ScriptedRunner {
                outputs: Mutex::new(outputs),
            },
        };

        let started_at = tokio::time::Instant::now();
        let result = systemd.restart_for_update(&expected_executable).await;
        let verified_for = started_at.elapsed();
        process.kill().unwrap();
        process.wait().unwrap();

        result.unwrap();
        assert_eq!(verified_for, Duration::from_secs(10));
    }

    #[tokio::test(start_paused = true)]
    async fn update_waits_for_the_service_process_to_exec_before_checking_stability() {
        use std::io::Write;

        struct StartingRunner {
            process: Mutex<std::process::Child>,
            expected: std::path::PathBuf,
            observations: std::sync::atomic::AtomicUsize,
        }

        impl CommandRunner for StartingRunner {
            async fn run(&self, arguments: &[&str]) -> Result<CommandOutput> {
                let mut process = self.process.lock().unwrap();
                if arguments[0] == "restart" {
                    return Ok(CommandOutput::success(""));
                }
                // The first observation is the launcher, before exec. Release it
                // only when verification asks for the process a second time.
                if self
                    .observations
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    > 0
                    && let Some(mut input) = process.stdin.take()
                {
                    input.write_all(b"start\n")?;
                    let deadline = std::time::Instant::now() + Duration::from_secs(5);
                    while std::fs::read_link(format!("/proc/{}/exe", process.id()))?
                        != self.expected
                    {
                        anyhow::ensure!(std::time::Instant::now() < deadline, "exec timed out");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                Ok(active_process(process.id()))
            }
        }

        impl Drop for StartingRunner {
            fn drop(&mut self) {
                let process = self.process.get_mut().unwrap();
                let _ = process.kill();
                let _ = process.wait();
            }
        }

        let process = StdCommand::new("sh")
            .args(["-c", "read line; exec sleep 30"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut target = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let expected = std::fs::read_link(format!("/proc/{}/exe", target.id())).unwrap();
        target.kill().unwrap();
        target.wait().unwrap();
        let systemd = Systemd {
            runner: StartingRunner {
                process: Mutex::new(process),
                expected: expected.clone(),
                observations: std::sync::atomic::AtomicUsize::new(0),
            },
        };
        let pid = systemd.runner.process.lock().unwrap().id();
        assert_ne!(
            std::fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
            expected
        );
        let started_at = tokio::time::Instant::now();
        systemd.restart_for_update(&expected).await.unwrap();
        assert_eq!(started_at.elapsed(), Duration::from_secs(11));
        assert_eq!(systemd.update_process().await.unwrap().unwrap().pid, pid);
    }

    #[tokio::test(start_paused = true)]
    async fn update_process_restart_during_verification_is_rejected() {
        let mut initial = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let mut replacement = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let expected_executable =
            std::fs::read_link(format!("/proc/{}/exe", initial.id())).unwrap();
        let systemd = Systemd {
            runner: ScriptedRunner {
                outputs: Mutex::new(VecDeque::from([
                    active_process(initial.id()),
                    active_process(replacement.id()),
                ])),
            },
        };

        let result = systemd
            .wait_for_stable_update_process(&expected_executable)
            .await;
        initial.kill().unwrap();
        initial.wait().unwrap();
        replacement.kill().unwrap();
        replacement.wait().unwrap();

        assert!(result.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn update_rejects_a_process_that_never_executes_the_installed_binary() {
        let systemd = Systemd {
            runner: ScriptedRunner {
                outputs: Mutex::new(
                    std::iter::repeat_with(|| active_process(std::process::id()))
                        .take(11)
                        .collect(),
                ),
            },
        };
        let started_at = tokio::time::Instant::now();
        let result = systemd
            .wait_for_stable_update_process(Path::new("/usr/local/bin/pontia-edge"))
            .await;
        assert!(result.is_err());
        assert_eq!(started_at.elapsed(), Duration::from_secs(10));
    }

    #[tokio::test(start_paused = true)]
    async fn update_rejects_a_replacement_pid_even_if_it_executes_the_installed_binary() {
        let mut launcher = StdCommand::new("sleep").arg("30").spawn().unwrap();
        let systemd = Systemd {
            runner: ScriptedRunner {
                outputs: Mutex::new(VecDeque::from([
                    active_process(launcher.id()),
                    active_process(std::process::id()),
                ])),
            },
        };
        let result = systemd
            .wait_for_stable_update_process(&std::env::current_exe().unwrap())
            .await;
        launcher.kill().unwrap();
        launcher.wait().unwrap();
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires root and a running systemd system manager"]
    async fn update_verifies_a_real_type_simple_service_after_restart() {
        struct TransientService {
            unit: String,
        }

        impl CommandRunner for TransientService {
            async fn run(&self, arguments: &[&str]) -> Result<CommandOutput> {
                let arguments: Vec<_> = arguments
                    .iter()
                    .map(|argument| {
                        if *argument == "pontia-edge.service" {
                            self.unit.as_str()
                        } else {
                            argument
                        }
                    })
                    .collect();
                ProcessCommandRunner.run(&arguments).await
            }
        }

        impl Drop for TransientService {
            fn drop(&mut self) {
                let _ = StdCommand::new("systemctl")
                    .args(["stop", &self.unit])
                    .status();
            }
        }

        assert!(rustix::process::geteuid().is_root());
        let root = tempfile::tempdir().unwrap();
        let unit = format!(
            "pontia-edge-update-test-{}.service",
            root.path().file_name().unwrap().to_str().unwrap()
        );
        let systemd = Systemd {
            runner: TransientService { unit },
        };
        let status = StdCommand::new("systemd-run")
            .args([
                "--quiet",
                "--collect",
                "--unit",
                &systemd.runner.unit,
                "--property=Type=simple",
                "--property=NoNewPrivileges=true",
                "--property=PrivateTmp=true",
                "--property=ProtectHome=true",
                "--property=ProtectSystem=strict",
                "/usr/bin/sleep",
                "300",
            ])
            .status()
            .unwrap();
        assert!(status.success());
        systemd
            .restart_for_update(Path::new("/usr/bin/sleep"))
            .await
            .unwrap();
        let process = systemd.update_process().await.unwrap().unwrap();
        assert_eq!(process.executable, Path::new("/usr/bin/sleep"));
    }

    #[tokio::test]
    async fn restart_failure_is_not_reported_as_success() {
        struct FailingRunner;
        impl CommandRunner for FailingRunner {
            async fn run(&self, _arguments: &[&str]) -> Result<CommandOutput> {
                Ok(CommandOutput {
                    success: false,
                    stdout: String::new(),
                })
            }
        }
        let systemd = Systemd {
            runner: FailingRunner,
        };
        assert!(!systemd.update_is_active().await.unwrap());
        assert!(systemd.restart().await.is_err());
    }

    #[tokio::test]
    async fn installs_the_embedded_unit_before_reloading_and_starting_systemd() {
        let test_root = tempfile::tempdir().unwrap();
        let unit_path = test_root.path().join("pontia-edge.service");
        let systemd = Systemd {
            runner: FakeRunner {
                calls: Mutex::new(Vec::new()),
            },
        };

        systemd.install_and_start(&unit_path).await.unwrap();

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
