use std::process::Command;

#[test]
fn server_rejects_invalid_pontia_home_before_startup_side_effects() {
    for (name, pontia_home) in [
        ("empty", ""),
        ("relative", "relative/pontia"),
        ("tilde", "~/.pontia"),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pontia"));
        command.env("PONTIA_HOME", pontia_home);
        let output = command.output().expect("run pontia server");
        assert!(!output.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("PONTIA_HOME"), "{name}: {stderr}");
        assert!(
            !stderr.contains("starting pontia control plane"),
            "{name}: {stderr}"
        );
    }
}
