use std::{
    fs,
    process::{Command, Stdio},
};

#[test]
fn incomplete_registration_parameters_are_rejected_before_configuration_changes() {
    for args in [
        vec![
            "remote",
            "enable",
            "--edge",
            "0195e7c1-1b22-7c33-9d44-123456789abc",
        ],
        vec!["remote", "enable", "--name", "Agent device"],
    ] {
        let root = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
            .args(args)
            .env("PONTIA_HOME", root.path())
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn noninteractive_remote_commands_require_login() {
    for args in [
        vec!["remote", "edges"],
        vec![
            "remote",
            "enable",
            "--edge",
            "0195e7c1-1b22-7c33-9d44-123456789abc",
            "--name",
            "Agent device",
        ],
    ] {
        let root = tempfile::tempdir().unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
            .args(args)
            .env("PONTIA_HOME", root.path())
            .env(
                "PONTIA_AUTH_ORIGIN",
                "https://must-not-be-requested.invalid",
            )
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
