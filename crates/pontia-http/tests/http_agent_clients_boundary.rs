use std::{path::Path, process::Command};

#[test]
fn http_crate_has_no_production_agent_adapter_dependencies() {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .output()
        .expect("read Cargo dependency metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid Cargo metadata");
    let package = metadata["packages"]
        .as_array()
        .expect("workspace packages")
        .iter()
        .find(|package| package["name"] == "pontia-http")
        .expect("pontia-http package");
    let offenders: Vec<_> = package["dependencies"]
        .as_array()
        .expect("HTTP crate dependencies")
        .iter()
        .filter(|dependency| dependency["kind"] != "dev")
        .filter_map(|dependency| dependency["name"].as_str())
        .filter(|name| name.starts_with("pontia-client-"))
        .collect();

    assert!(
        offenders.is_empty(),
        "HTTP must depend on application contracts instead of agent adapters: {}",
        offenders.join(", ")
    );
}
