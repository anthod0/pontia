use std::process::Command;

fn pontiactl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pontiactl"))
}

#[test]
fn starts_without_a_command() {
    let output = pontiactl().output().expect("run pontiactl");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn help_describes_the_cli() {
    let output = pontiactl().arg("--help").output().expect("run pontiactl");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is utf-8");
    assert!(stdout.contains("Control Pontia from the command line"));
    assert!(stdout.contains("Usage: pontiactl"));
}

#[test]
fn version_reports_the_workspace_version() {
    let output = pontiactl()
        .arg("--version")
        .output()
        .expect("run pontiactl");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("version is utf-8"),
        format!("pontiactl {}\n", env!("CARGO_PKG_VERSION"))
    );
}
