use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use pontia::{
    codex::{CodexDaemonProbe, CodexSetup, initialize, inspect},
    lifecycle::DefinitionStore,
    manager::{CommandOutput, CommandRunner},
};

#[derive(Debug, PartialEq, Eq)]
struct Call {
    program: String,
    args: Vec<String>,
    environment: Vec<(String, String)>,
}

#[derive(Default)]
struct FakeRunner {
    outputs: RefCell<VecDeque<CommandOutput>>,
    calls: RefCell<Vec<Call>>,
}

impl FakeRunner {
    fn with_outputs(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: RefCell::new(outputs.into()),
            calls: RefCell::new(Vec::new()),
        }
    }

    fn record(
        &self,
        program: &str,
        args: &[String],
        environment: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        self.calls.borrow_mut().push(Call {
            program: program.to_string(),
            args: args.to_vec(),
            environment: environment.to_vec(),
        });
        self.outputs
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| "unexpected command".to_string())
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        self.record(program, args, &[])
    }

    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        environment: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        self.record(program, args, environment)
    }
}

#[derive(Default)]
struct FakeStore {
    installs: RefCell<Vec<(PathBuf, String)>>,
    error: Option<&'static str>,
}

impl DefinitionStore for FakeStore {
    fn read(&self, _path: &Path) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn install(&self, path: &Path, contents: &str) -> Result<bool, String> {
        self.installs
            .borrow_mut()
            .push((path.to_path_buf(), contents.to_string()));
        self.error.map_or(Ok(true), |error| Err(error.to_string()))
    }
}

#[derive(Default)]
struct FakeProbe {
    homes: RefCell<Vec<PathBuf>>,
    error: Option<&'static str>,
}

impl CodexDaemonProbe for FakeProbe {
    fn probe(&self, codex_home: &Path) -> Result<(), String> {
        self.homes.borrow_mut().push(codex_home.to_path_buf());
        self.error.map_or(Ok(()), |error| Err(error.to_string()))
    }
}

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, "#!/bin/sh\n").expect("write executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("set executable mode");
}

#[test]
fn inspection_resolves_absolute_paths_and_checks_required_commands() {
    let test_root = tempfile::tempdir().expect("temp dir");
    let user_home = test_root.path().join("home");
    let codex_home = test_root.path().join("custom-codex");
    let bin = test_root.path().join("bin");
    fs::create_dir_all(&user_home).expect("create user home");
    fs::create_dir(&codex_home).expect("create Codex home");
    fs::create_dir(&bin).expect("create bin");
    make_executable(&bin.join("codex"));
    let runner = FakeRunner::with_outputs(vec![
        output(0, "alice\n", ""),
        output(0, "codex-cli 1.0\n", ""),
        output(0, "start help\n", ""),
        output(0, "version help\n", ""),
    ]);
    let vars = HashMap::from([
        ("PATH".to_string(), bin.display().to_string()),
        ("CODEX_HOME".to_string(), codex_home.display().to_string()),
    ]);

    let setup = inspect(&vars, &user_home, &runner).expect("inspect Codex");

    assert_eq!(setup.executable, bin.join("codex"));
    assert_eq!(setup.home, codex_home.canonicalize().unwrap());
    assert_eq!(setup.username, "alice");
    assert_eq!(
        setup.service_path,
        user_home.join(".config/systemd/user/pontia-codex.service")
    );
    let calls = runner.calls.into_inner();
    assert_eq!(calls[0].program, "id");
    assert_eq!(calls[0].args, ["-un"]);
    assert_eq!(calls[1].program, bin.join("codex").to_str().unwrap());
    assert_eq!(
        calls[1].environment,
        [("CODEX_HOME".to_string(), codex_home.display().to_string())]
    );
    assert_eq!(calls[2].args, ["app-server", "daemon", "start", "--help"]);
    assert_eq!(calls[3].args, ["app-server", "daemon", "version", "--help"]);
}

#[test]
fn inspection_accepts_a_fresh_default_codex_home() {
    let test_root = tempfile::tempdir().expect("temp dir");
    let user_home = test_root.path().join("home");
    let bin = test_root.path().join("bin");
    fs::create_dir(&user_home).expect("create user home");
    fs::create_dir(&bin).expect("create bin");
    make_executable(&bin.join("codex"));
    let runner = FakeRunner::with_outputs(vec![
        output(0, "alice\n", ""),
        output(0, "codex-cli 1.0\n", ""),
        output(0, "start help\n", ""),
        output(0, "version help\n", ""),
    ]);
    let vars = HashMap::from([("PATH".to_string(), bin.display().to_string())]);

    let setup = inspect(&vars, &user_home, &runner).expect("inspect fresh Codex home");

    assert_eq!(setup.home, user_home.join(".codex"));
    assert!(!setup.home.exists());
}

#[test]
fn inspection_reports_missing_executable_and_unsupported_daemon_commands() {
    let test_root = tempfile::tempdir().expect("temp dir");
    let user_home = test_root.path().join("home");
    let empty_bin = test_root.path().join("empty-bin");
    fs::create_dir(&user_home).expect("create user home");
    fs::create_dir(&empty_bin).expect("create empty bin");
    let vars = HashMap::from([("PATH".to_string(), empty_bin.display().to_string())]);
    let error = inspect(&vars, &user_home, &FakeRunner::default())
        .expect_err("missing Codex must fail inspection");
    assert!(error.contains("installed and executable"));

    make_executable(&empty_bin.join("codex"));
    let runner = FakeRunner::with_outputs(vec![
        output(0, "alice\n", ""),
        output(0, "codex-cli 1.0\n", ""),
        output(2, "", "unknown command start"),
    ]);
    let error = inspect(&vars, &user_home, &runner)
        .expect_err("unsupported daemon start must fail inspection");
    assert!(error.contains("Codex daemon start capability check"));
    assert!(error.contains("unknown command start"));
}

#[test]
fn initialization_installs_enables_and_verifies_codex_without_restarting_it() {
    let test_root = tempfile::tempdir().expect("temp dir");
    let setup = CodexSetup {
        executable: PathBuf::from("/opt/codex/bin/codex"),
        home: test_root.path().to_path_buf(),
        username: "alice".to_string(),
        service_path: test_root.path().join("pontia-codex.service"),
    };
    let runner = FakeRunner::with_outputs(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
        output(0, r#"{"status":"running","appServerVersion":"1.0"}"#, ""),
        output(0, "enabled\n", ""),
        output(0, "yes\n", ""),
    ]);
    let store = FakeStore::default();
    let probe = FakeProbe::default();

    initialize(&setup, &runner, &store, &probe).expect("initialize Codex");

    let installs = store.installs.into_inner();
    assert_eq!(installs[0].0, setup.service_path);
    assert!(installs[0].1.contains("Type=oneshot"));
    assert!(installs[0].1.contains("app-server daemon start"));
    assert!(installs[0].1.contains(&format!(
        "Environment=\"CODEX_HOME={}\"",
        setup.home.display()
    )));
    assert_eq!(
        probe.homes.borrow().as_slice(),
        std::slice::from_ref(&setup.home)
    );

    let calls = runner.calls.into_inner();
    assert_eq!(
        calls
            .iter()
            .map(|call| call.args.clone())
            .collect::<Vec<_>>(),
        vec![
            vec!["--user", "daemon-reload"],
            vec!["enable-linger", "alice"],
            vec!["--user", "enable", "--now", "pontia-codex.service"],
            vec!["app-server", "daemon", "version"],
            vec!["--user", "is-enabled", "pontia-codex.service"],
            vec!["show-user", "alice", "--property=Linger", "--value"],
        ]
    );
    assert!(
        calls
            .iter()
            .all(|call| !call.args.iter().any(|arg| arg == "restart"))
    );
}

#[test]
fn initialization_reports_service_definition_and_linger_failures() {
    let test_root = tempfile::tempdir().expect("temp dir");
    let setup = CodexSetup {
        executable: PathBuf::from("/opt/codex"),
        home: test_root.path().to_path_buf(),
        username: "alice".to_string(),
        service_path: test_root.path().join("pontia-codex.service"),
    };
    let store = FakeStore {
        error: Some("read-only filesystem"),
        ..FakeStore::default()
    };
    let error = initialize(
        &setup,
        &FakeRunner::default(),
        &store,
        &FakeProbe::default(),
    )
    .expect_err("definition installation must fail");
    assert!(error.contains("Codex service definition installation failed"));
    assert!(error.contains("read-only filesystem"));

    let runner =
        FakeRunner::with_outputs(vec![output(0, "", ""), output(1, "", "permission denied")]);
    let error = initialize(
        &setup,
        &runner,
        &FakeStore::default(),
        &FakeProbe::default(),
    )
    .expect_err("linger enablement must fail");
    assert!(error.contains("Codex linger enablement failed"));
    assert!(error.contains("permission denied"));

    let runner = FakeRunner::with_outputs(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(1, "", "unit could not be enabled"),
    ]);
    let error = initialize(
        &setup,
        &runner,
        &FakeStore::default(),
        &FakeProbe::default(),
    )
    .expect_err("service enablement must fail");
    assert!(error.contains("Codex service enablement failed"));
    assert!(error.contains("unit could not be enabled"));
}

#[test]
fn initialization_reports_the_failed_verification_step() {
    let test_root = tempfile::tempdir().expect("temp dir");
    let setup = CodexSetup {
        executable: PathBuf::from("/opt/codex"),
        home: test_root.path().to_path_buf(),
        username: "alice".to_string(),
        service_path: test_root.path().join("pontia-codex.service"),
    };
    let runner = FakeRunner::with_outputs(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
        output(0, r#"{"status":"running"}"#, ""),
        output(0, "enabled\n", ""),
        output(0, "yes\n", ""),
    ]);
    let probe = FakeProbe {
        error: Some("handshake rejected"),
        ..FakeProbe::default()
    };

    let error = initialize(&setup, &runner, &FakeStore::default(), &probe)
        .expect_err("protocol verification must fail initialization");

    assert!(error.contains("Codex protocol connection check failed"));
    assert!(error.contains("handshake rejected"));
}
