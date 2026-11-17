use pontia_runtime::client_setup::{SetupStatus, latest, run_worker};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn executable(root: &Path, script: &str) -> std::path::PathBuf {
    let path = root.join("fake-pontia");
    fs::write(
        &path,
        format!("#!/bin/sh\n[ \"$1\" = setup ] && [ \"$2\" = sample ] || exit 90\n{script}\n"),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
fn setup_records_the_real_exit_result_and_combined_output_for_later_readers() {
    for (exit, status) in [(0, SetupStatus::Succeeded), (7, SetupStatus::Failed)] {
        let root = tempfile::tempdir().unwrap();
        let cli = executable(
            root.path(),
            &format!("echo installed; echo diagnostic >&2; exit {exit}"),
        );
        let mut receipt = Vec::new();
        run_worker(
            root.path(),
            &cli,
            "test-client",
            "default",
            "sample",
            "first",
            &mut receipt,
        )
        .unwrap();
        let accepted: Result<pontia_runtime::client_setup::SetupOperation, String> =
            serde_json::from_slice(&receipt).unwrap();
        assert_eq!(accepted.unwrap().status, SetupStatus::Running);
        let result = latest(root.path()).unwrap().unwrap();
        assert_eq!(result.status, status);
        assert!(result.output.contains("installed\ndiagnostic"));
        assert_eq!(result.error.is_some(), exit != 0);
        assert_eq!(latest(root.path()).unwrap().unwrap().status, status);
    }
}

#[test]
fn concurrent_setup_is_rejected_and_a_later_retry_can_run() {
    let root = tempfile::tempdir().unwrap();
    let cli = executable(
        root.path(),
        "for attempt in $(seq 1 100); do [ -e \"$PONTIA_HOME/release\" ] && break; sleep 0.05; done\necho ready",
    );
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            run_worker(
                root.path(),
                &cli,
                "test-client",
                "default",
                "sample",
                "first",
                &mut Vec::new(),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while latest(root.path()).unwrap().is_none() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let duplicate = run_worker(
            root.path(),
            &cli,
            "test-client",
            "default",
            "sample",
            "second",
            &mut Vec::new(),
        );
        // Release the subprocess before assertions so a failure cannot strand the worker.
        fs::write(root.path().join("release"), "").unwrap();
        worker.join().unwrap().unwrap();
        assert!(duplicate.is_err());
        assert_eq!(latest(root.path()).unwrap().unwrap().id, "first");
    });
    run_worker(
        root.path(),
        &cli,
        "test-client",
        "default",
        "sample",
        "retry",
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(latest(root.path()).unwrap().unwrap().id, "retry");
}

#[test]
fn setup_worker_process() {
    let Some(root) = std::env::var_os("PONTIA_SETUP_WORKER_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    run_worker(
        &root,
        &root.join("fake-pontia"),
        "test-client",
        "default",
        "sample",
        "interrupted",
        &mut Vec::new(),
    )
    .unwrap();
}

#[test]
fn an_exited_worker_is_reported_as_failed_instead_of_running_forever() {
    let root = tempfile::tempdir().unwrap();
    executable(
        root.path(),
        "echo ready > \"$PONTIA_HOME/started\"; for attempt in $(seq 1 100); do [ -e \"$PONTIA_HOME/release\" ] && break; sleep 0.05; done\necho done > \"$PONTIA_HOME/finished\"",
    );
    let mut worker = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "setup_worker_process"])
        .env("PONTIA_SETUP_WORKER_TEST_ROOT", root.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.path().join("started").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let initial = latest(root.path()).unwrap();
    worker.kill().unwrap();
    worker.wait().unwrap();
    fs::write(root.path().join("release"), "").unwrap();
    while !root.path().join("finished").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let result = latest(root.path()).unwrap().unwrap();
    assert_eq!(initial.unwrap().status, SetupStatus::Running);
    assert_eq!(result.status, SetupStatus::Failed);
    assert!(result.error.is_some());
}
