use std::{path::PathBuf, process::Command, sync::OnceLock};

// Cargo only provides CARGO_BIN_EXE for binaries in the package owning the test.
pub fn executable() -> PathBuf {
    static RELAY: OnceLock<PathBuf> = OnceLock::new();
    RELAY
        .get_or_init(|| {
            let build = Command::new(env!("CARGO"))
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args([
                    "build",
                    "--locked",
                    "-p",
                    "pontia-client-acp",
                    "--bin",
                    "pontia-acp-test-relay",
                    "--message-format=json",
                ])
                .output()
                .expect("build ACP integration test relay");
            assert!(
                build.status.success(),
                "ACP relay build failed: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            String::from_utf8(build.stdout)
                .unwrap()
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find_map(|artifact| {
                    (artifact["target"]["name"] == "pontia-acp-test-relay")
                        .then(|| artifact["executable"].as_str().map(PathBuf::from))
                        .flatten()
                })
                .expect("ACP relay executable artifact")
        })
        .clone()
}
