use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn auto_update_status_prints_the_timer_schedule_and_last_update_result() {
    let root = tempfile::tempdir().unwrap();
    let systemctl = root.path().join("systemctl");
    fs::write(
        &systemctl,
        r#"#!/bin/sh
case "$1:$2" in
    show:pontia-edge-update.timer)
        printf '%s\n' 'UnitFileState=enabled' 'NextElapseUSecRealtime=Sun 2026-11-08 15:07:08 UTC'
        ;;
    show:pontia-edge-update.service)
        printf '%s\n' 'Result=exit-code' 'ExecMainStatus=1'
        ;;
    *) exit 1 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&systemctl, fs::Permissions::from_mode(0o700)).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pontia-edge"))
        .args(["auto-update", "status"])
        .env("PATH", root.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let properties: std::collections::HashMap<_, _> = stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    assert_eq!(properties.get("UnitFileState"), Some(&"enabled"));
    assert_eq!(
        properties.get("NextElapseUSecRealtime"),
        Some(&"Sun 2026-11-08 15:07:08 UTC")
    );
    assert_eq!(properties.get("Result"), Some(&"exit-code"));
    assert_eq!(properties.get("ExecMainStatus"), Some(&"1"));
}
