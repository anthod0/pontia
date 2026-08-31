use std::{cell::RefCell, collections::VecDeque, path::Path};

use pontia::{
    lifecycle::{EnabledState, RunState, ServiceManager, ServiceStatus},
    manager::{CommandOutput, CommandRunner, LaunchdManager, SystemdManager},
};

#[derive(Default)]
struct FakeRunner {
    outputs: RefCell<VecDeque<CommandOutput>>,
    calls: RefCell<Vec<(String, Vec<String>)>>,
}

impl FakeRunner {
    fn with_outputs(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: RefCell::new(outputs.into()),
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        self.calls
            .borrow_mut()
            .push((program.to_string(), args.to_vec()));
        self.outputs
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| "unexpected command".to_string())
    }
}

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

#[test]
fn systemd_reports_normalized_status() {
    let runner = FakeRunner::with_outputs(vec![
        output(0, "enabled\n", ""),
        output(0, "LoadState=loaded\nActiveState=active\n", ""),
    ]);
    let manager = SystemdManager::new(&runner);

    assert_eq!(
        manager.status().expect("status succeeds"),
        ServiceStatus {
            enabled: EnabledState::Enabled,
            loaded: true,
            run_state: RunState::Running,
        }
    );
}

#[test]
fn systemd_failure_diagnostic_includes_the_service_status_output() {
    let runner = FakeRunner::with_outputs(vec![output(
        3,
        "× pontia.service - Pontia Control Plane\n  Error: Address already in use\n",
        "",
    )]);
    let manager = SystemdManager::new(&runner);

    let diagnostic = manager
        .failure_diagnostic()
        .expect("failure diagnostic succeeds");

    assert!(
        diagnostic.contains("Address already in use"),
        "{diagnostic}"
    );
    assert_eq!(
        runner.calls.into_inner(),
        vec![(
            "systemctl".to_string(),
            vec!["--user", "status", "pontia.service", "--no-pager", "--full"]
                .into_iter()
                .map(String::from)
                .collect()
        )]
    );
}

#[test]
fn systemd_reloads_enables_and_restarts_changed_running_service() {
    let runner = FakeRunner::with_outputs(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
    ]);
    let manager = SystemdManager::new(&runner);

    manager
        .apply(
            Path::new("/home/alice/.config/systemd/user/pontia.service"),
            true,
            true,
            ServiceStatus {
                enabled: EnabledState::Enabled,
                loaded: true,
                run_state: RunState::Running,
            },
        )
        .expect("apply succeeds");

    assert_eq!(
        runner.calls.into_inner(),
        vec![
            (
                "systemctl".to_string(),
                vec!["--user", "daemon-reload"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
            (
                "systemctl".to_string(),
                vec!["--user", "enable", "--now", "pontia.service"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
            (
                "systemctl".to_string(),
                vec!["--user", "restart", "pontia.service"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
        ]
    );
}

#[test]
fn systemd_restarts_running_service_when_lifecycle_requests_it() {
    let runner = FakeRunner::with_outputs(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
    ]);
    let manager = SystemdManager::new(&runner);

    manager
        .apply(
            Path::new("/home/alice/.config/systemd/user/pontia.service"),
            false,
            true,
            ServiceStatus {
                enabled: EnabledState::Enabled,
                loaded: true,
                run_state: RunState::Running,
            },
        )
        .expect("apply succeeds");

    assert_eq!(
        runner.calls.into_inner().last(),
        Some(&(
            "systemctl".to_string(),
            vec!["--user", "restart", "pontia.service"]
                .into_iter()
                .map(String::from)
                .collect(),
        ))
    );
}

#[test]
fn systemd_down_accepts_already_missing_service() {
    let runner = FakeRunner::with_outputs(vec![output(
        1,
        "",
        "Failed to disable unit: Unit file pontia.service does not exist.",
    )]);
    SystemdManager::new(&runner)
        .down()
        .expect("missing service is already down");
}

#[test]
fn launchd_reports_loaded_failed_service_and_disabled_override() {
    let runner = FakeRunner::with_outputs(vec![
        output(
            0,
            "disabled services = {\n  \"dev.pontia.pontiad\" => true\n}\n",
            "",
        ),
        output(
            0,
            "gui/501/dev.pontia.pontiad = {\n state = exited\n last exit code = 78\n}\n",
            "",
        ),
    ]);
    let manager = LaunchdManager::new(&runner, 501);

    assert_eq!(
        manager.status().expect("status succeeds"),
        ServiceStatus {
            enabled: EnabledState::Disabled,
            loaded: true,
            run_state: RunState::Failed,
        }
    );
}

#[test]
fn launchd_failure_diagnostic_includes_the_service_status_output() {
    let runner = FakeRunner::with_outputs(vec![output(
        0,
        "gui/501/dev.pontia.pontiad = {\n state = exited\n last exit code = 48\n}\n",
        "",
    )]);
    let manager = LaunchdManager::new(&runner, 501);

    let diagnostic = manager
        .failure_diagnostic()
        .expect("failure diagnostic succeeds");

    assert!(diagnostic.contains("last exit code = 48"), "{diagnostic}");
    assert_eq!(
        runner.calls.into_inner(),
        vec![(
            "launchctl".to_string(),
            vec!["print", "gui/501/dev.pontia.pontiad"]
                .into_iter()
                .map(String::from)
                .collect()
        )]
    );
}

#[test]
fn launchd_replaces_changed_loaded_definition() {
    let runner = FakeRunner::with_outputs(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
    ]);
    let manager = LaunchdManager::new(&runner, 501);

    manager
        .apply(
            Path::new("/Users/alice/Library/LaunchAgents/dev.pontia.pontiad.plist"),
            true,
            true,
            ServiceStatus {
                enabled: EnabledState::Enabled,
                loaded: true,
                run_state: RunState::Running,
            },
        )
        .expect("apply succeeds");

    assert_eq!(
        runner.calls.into_inner(),
        vec![
            (
                "launchctl".to_string(),
                vec!["enable", "gui/501/dev.pontia.pontiad"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
            (
                "launchctl".to_string(),
                vec!["bootout", "gui/501/dev.pontia.pontiad"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
            (
                "launchctl".to_string(),
                vec![
                    "bootstrap",
                    "gui/501",
                    "/Users/alice/Library/LaunchAgents/dev.pontia.pontiad.plist"
                ]
                .into_iter()
                .map(String::from)
                .collect()
            ),
        ]
    );
}

#[test]
fn launchd_restarts_loaded_service_when_lifecycle_requests_it() {
    let runner = FakeRunner::with_outputs(vec![output(0, "", ""), output(0, "", "")]);
    let manager = LaunchdManager::new(&runner, 501);

    manager
        .apply(
            Path::new("/Users/alice/Library/LaunchAgents/dev.pontia.pontiad.plist"),
            false,
            true,
            ServiceStatus {
                enabled: EnabledState::Enabled,
                loaded: true,
                run_state: RunState::Running,
            },
        )
        .expect("apply succeeds");

    assert_eq!(
        runner.calls.into_inner(),
        vec![
            (
                "launchctl".to_string(),
                vec!["enable", "gui/501/dev.pontia.pontiad"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
            (
                "launchctl".to_string(),
                vec!["kickstart", "-k", "gui/501/dev.pontia.pontiad"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            ),
        ]
    );
}

#[test]
fn launchd_down_is_idempotent_when_service_is_not_loaded() {
    let runner = FakeRunner::with_outputs(vec![
        output(0, "disabled services = {\n}\n", ""),
        output(
            113,
            "",
            "Could not find service \"dev.pontia.pontiad\" in domain for user gui: 501",
        ),
        output(0, "", ""),
    ]);
    let manager = LaunchdManager::new(&runner, 501);

    manager.down().expect("already unloaded service is down");

    assert_eq!(
        runner.calls.into_inner().last(),
        Some(&(
            "launchctl".to_string(),
            vec!["disable", "gui/501/dev.pontia.pontiad"]
                .into_iter()
                .map(String::from)
                .collect(),
        ))
    );
}

#[test]
fn managers_extract_persisted_home_from_their_rendered_definition() {
    let runner = FakeRunner::default();
    let systemd = SystemdManager::new(&runner);
    let unit = systemd
        .render_definition(
            Path::new("/opt/pontiad"),
            Path::new("/home/a/Pontia % home"),
        )
        .unwrap();
    assert_eq!(
        systemd.persisted_home(&unit).unwrap(),
        Path::new("/home/a/Pontia % home")
    );

    let launchd = LaunchdManager::new(&runner, 501);
    let plist = launchd
        .render_definition(
            Path::new("/opt/pontiad"),
            Path::new("/Users/a/Pontia & home"),
        )
        .unwrap();
    assert_eq!(
        launchd.persisted_home(&plist).unwrap(),
        Path::new("/Users/a/Pontia & home")
    );
}
