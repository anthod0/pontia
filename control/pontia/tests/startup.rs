use std::process::Command;

#[test]
fn server_rejects_invalid_pontia_home_before_startup_side_effects() {
    for (name, pontia_home) in [
        ("missing", None),
        ("empty", Some("")),
        ("relative", Some("relative/pontia")),
        ("tilde", Some("~/.pontia")),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pontia"));
        match pontia_home {
            Some(value) => {
                command.env("PONTIA_HOME", value);
            }
            None => {
                command.env_remove("PONTIA_HOME");
            }
        }
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
