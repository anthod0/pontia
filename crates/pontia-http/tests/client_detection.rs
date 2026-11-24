use pontia_agent_control::client_contract::{
    ClientConfiguration, DetectionContext, DetectionStatus,
};
use serde_json::json;
use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, path::Path};

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn pi_checks_effective_command_without_executing_it() {
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("custom-pi");
    let vars = HashMap::from([("PATH".into(), root.path().display().to_string())]);
    let context = DetectionContext {
        vars: &vars,
        pontia_home: root.path(),
    };
    let config = pontia_client_pi::config::PiConfiguration::new(
        json!({"tui_command":"custom-pi"}),
        json!({}),
    );
    let effective = config.merged("default");
    assert_eq!(
        config.detect(&effective, &context).status,
        DetectionStatus::NotDetected
    );
    executable(&program, "#!/bin/sh\nexit 99\n");
    assert_eq!(
        config.detect(&effective, &context).status,
        DetectionStatus::Detected
    );
    fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        config.detect(&effective, &context).status,
        DetectionStatus::NotDetected
    );
    assert_eq!(
        config
            .detect(&json!({"tui_command":"env KEY=value pi"}), &context)
            .status,
        DetectionStatus::Unknown
    );
    assert_eq!(
        config.detect(&json!({"tui_command":null}), &context).status,
        DetectionStatus::Unknown
    );
}

#[test]
fn codex_detection_skips_pontia_launcher_and_tracks_native_executable() {
    let root = tempfile::tempdir().unwrap();
    let shim = root.path().join("shim");
    let native = root.path().join("native");
    fs::create_dir(&shim).unwrap();
    fs::create_dir(&native).unwrap();
    executable(
        &shim.join("codex"),
        "#!/bin/sh\n# Pontia Codex launcher\nexit 99\n",
    );
    let vars = HashMap::from([(
        "PATH".into(),
        format!("{}:{}", shim.display(), native.display()),
    )]);
    let context = DetectionContext {
        vars: &vars,
        pontia_home: root.path(),
    };
    let integration = pontia_client_codex::setup::integration();
    let config = pontia_client_codex::configuration::CodexConfiguration;
    assert_eq!(
        integration.detect(&context).status,
        DetectionStatus::NotDetected
    );
    executable(&native.join("codex"), "#!/bin/sh\nexit 99\n");
    assert_eq!(
        integration.detect(&context).status,
        DetectionStatus::Detected
    );
    assert_eq!(
        config.detect(&json!({}), &context).status,
        DetectionStatus::Detected
    );
    fs::remove_file(native.join("codex")).unwrap();
    assert_eq!(
        config.detect(&json!({}), &context).status,
        DetectionStatus::NotDetected
    );
}

#[test]
fn claude_detects_native_client_or_sdk_host_with_node() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([("PATH".into(), root.path().display().to_string())]);
    let context = DetectionContext {
        vars: &vars,
        pontia_home: root.path(),
    };
    let integration = pontia_client_claude::setup::integration("1.0.0");
    assert_eq!(
        integration.detect(&context).status,
        DetectionStatus::NotDetected
    );
    executable(&root.path().join("claude"), "#!/bin/sh\nexit 99\n");
    assert_eq!(
        integration.detect(&context).status,
        DetectionStatus::Detected
    );
    fs::remove_file(root.path().join("claude")).unwrap();
    let host = root.path().join("host.js");
    fs::write(&host, "throw new Error('must not run');").unwrap();
    let registration = pontia_client_claude::registration_with_host(Some(host));
    let config = registration.configuration.unwrap();
    assert_eq!(
        config.detect(&json!({}), &context).status,
        DetectionStatus::NotDetected
    );
    executable(&root.path().join("node"), "#!/bin/sh\nexit 99\n");
    assert_eq!(
        config.detect(&json!({}), &context).status,
        DetectionStatus::Detected
    );
    assert_eq!(
        integration.detect(&context).status,
        DetectionStatus::NotDetected
    );
}

#[test]
fn acp_detects_the_executable_from_its_definition_on_path() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([("PATH".into(), root.path().display().to_string())]);
    let context = DetectionContext {
        vars: &vars,
        pontia_home: root.path(),
    };
    let config = pontia_client_acp::configuration::AcpConfiguration::new(
        pontia_client_acp::AcpAgents::builtin(),
    );
    let effective = config.resolve("grok").unwrap();
    assert_eq!(
        config.detect(&effective, &context).status,
        DetectionStatus::NotDetected
    );
    executable(&root.path().join("grok"), "#!/bin/sh\nexit 99\n");
    assert_eq!(
        config.detect(&effective, &context).status,
        DetectionStatus::Detected
    );
}
