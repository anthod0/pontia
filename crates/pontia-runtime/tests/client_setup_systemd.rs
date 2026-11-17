#![cfg(target_os = "linux")]
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn setup_parent_service() {
    let Some(root) = env::var_os("PONTIA_SETUP_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let operation =
        pontia_runtime::client_setup::start(&root, &root.join("cli"), "test", "operation").unwrap();
    assert_eq!(operation.id, "operation");
    fs::write(root.join("accepted"), "").unwrap();
    thread::sleep(Duration::from_secs(30));
}

struct Fixture {
    root: tempfile::TempDir,
    unit: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::write(self.root.path().join("release"), "");
        let _ = Command::new("systemctl")
            .args(["--user", "stop", &self.unit])
            .output();
    }
}

#[test]
fn setup_subprocess_survives_parent_service_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let unit = format!(
        "pontia-setup-test-{}.service",
        root.path().file_name().unwrap().to_str().unwrap()
    );
    let fixture = Fixture { root, unit };
    let cli = fixture.root.path().join("cli");
    fs::write(&cli, r##"#!/bin/sh
[ "$1" = setup-worker ] && [ "$2" = test ] || exit 90
printf '%s\n' '{"Ok":{"id":"operation","client_type":"test","client_key":"default","status":"running","error":null,"output":""}}'
cat /proc/self/cgroup > "$PONTIA_HOME/worker-cgroup"
for attempt in $(seq 1 300); do
  if [ -e "$PONTIA_HOME/release" ]; then
    printf 'completed' > "$PONTIA_HOME/completed"
    exit 0
  fi
  sleep 0.05
done
exit 91
"##).unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new("systemd-run")
        .args([
            "--user",
            "--quiet",
            "--collect",
            "--property=Type=exec",
            "--property=KillMode=control-group",
            "--unit",
            &fixture.unit,
        ])
        .arg(format!(
            "--setenv=PONTIA_SETUP_TEST_ROOT={}",
            fixture.root.path().display()
        ))
        .arg("--")
        .arg(env::current_exe().unwrap())
        .args(["--exact", "setup_parent_service", "--nocapture"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wait = |file: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !fixture.root.path().join(file).exists() {
            assert!(Instant::now() < deadline, "Missing {file}");
            thread::sleep(Duration::from_millis(20));
        }
    };
    wait("accepted");
    wait("worker-cgroup");
    let group = fs::read_to_string(fixture.root.path().join("worker-cgroup")).unwrap();
    assert!(!group.contains(&fixture.unit));
    let output = Command::new("systemctl")
        .args(["--user", "stop", &fixture.unit])
        .output()
        .unwrap();
    assert!(output.status.success());
    fs::write(fixture.root.path().join("release"), "").unwrap();
    wait("completed");
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("completed")).unwrap(),
        "completed"
    );
}
