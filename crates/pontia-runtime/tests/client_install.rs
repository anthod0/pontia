use pontia_runtime::client_operations::{self as operations, PackageManager, Phase, Status};
use std::{fs, io, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn state_and_log_survive_worker_completion() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    assert!(operations::latest(home).unwrap().is_none());
    let installed = operations::run(home, "client", "variant", "test", |op| {
        let error = operations::run(home, "other", "key", "method", |_| Ok(())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(op.advance(Phase::Verifying).is_err());
        op.advance(Phase::Installing)?;
        op.log("one")?;
        let start = operations::read_log(home, op.id(), 0)?;
        assert_eq!(start, b"one\n");
        op.log("two")?;
        assert_eq!(
            operations::read_log(home, op.id(), start.len() as u64)?,
            b"two\n"
        );
        op.advance(Phase::Verifying)
    })
    .unwrap();
    assert_eq!(installed.status, Status::Succeeded);
    assert_eq!(
        operations::latest(home).unwrap().unwrap().phase,
        Phase::Done
    );
    assert!(operations::cancel(home, installed.id).is_err());
    let failed = operations::run(home, "other", "key", "test", |_| {
        Err(io::Error::other("preflight"))
    })
    .unwrap();
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(
        operations::state(home, installed.id).unwrap().status,
        Status::Succeeded
    );
    let dir = home.join("state/client-operations");
    for ext in ["log", "json"] {
        assert_eq!(
            dir.join(format!("{}.{ext}", installed.id))
                .metadata()
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn stale_worker_identity_is_reconciled_without_signalling_the_pid() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let done = operations::run(home, "client", "key", "test", |_| Ok(())).unwrap();
    let file = home.join(format!("state/client-operations/{}.json", done.id));
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    data["status"] = "running".into();
    data["worker"]["start_time_seconds"] = 0.into();
    fs::write(&file, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(operations::cancel(home, done.id).is_err());
    assert_eq!(
        operations::latest(home).unwrap().unwrap().status,
        Status::Failed
    );
}

#[test]
fn installs_managed_packages_and_preserves_existing_targets() {
    // Spawn an isolated test process so other tests never inherit the fixture PATH.
    if std::env::var_os("PONTIA_TEST_PACKAGE_CHILD").is_some() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        for (manager, name) in [(PackageManager::Npm, "npm"), (PackageManager::Bun, "bun")] {
            let result = operations::run(home, "client", name, name, |op| {
                op.advance(Phase::Installing)?;
                op.install_package(manager, name, "@example/tool", name, true)?;
                op.advance(Phase::Verifying)
            })
            .unwrap();
            assert_eq!(result.status, Status::Succeeded, "{:?}", result.error);
            let link = home.join("tools/bin").join(name);
            assert_eq!(
                link.canonicalize().unwrap(),
                home.join("tools")
                    .join(name)
                    .join("node_modules/.bin")
                    .join(name)
            );
            let retry = operations::run(home, "client", name, name, |op| {
                op.advance(Phase::Installing)?;
                op.install_package(manager, name, "@example/tool", name, true)
            })
            .unwrap();
            assert_eq!(retry.status, Status::Failed);
            assert!(link.exists());
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for name in ["npm", "bun"] {
        let path = root.path().join(name);
        fs::write(
            &path,
            r#"#!/bin/sh
set -eu
case " $* " in *" --ignore-scripts -- @example/tool ") ;; *) exit 1 ;; esac
test -z "${PONTIA_EXTERNAL_API_TOKEN:-}"
test "$CI" = 1
mkdir -p node_modules/.bin
touch "node_modules/.bin/${0##*/}"
chmod +x "node_modules/.bin/${0##*/}"
"#,
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "installs_managed_packages_and_preserves_existing_targets",
        ])
        .env("PATH", format!("{}:/usr/bin:/bin", root.path().display()))
        .env("PONTIA_EXTERNAL_API_TOKEN", "test-credential")
        .env("PONTIA_TEST_PACKAGE_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success());
}
