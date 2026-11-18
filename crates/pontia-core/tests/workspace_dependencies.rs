use std::{path::Path, process::Command};

use serde::Deserialize;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    name: String,
    kind: Option<String>,
}

fn forbidden_dependency(source: &str, target: &str) -> Option<&'static str> {
    if target.starts_with("pontia-client-") && source != "pontia-clients" {
        return Some("Concrete Agent Clients are assembled only by pontia-clients");
    }

    if target == "pontia-clients" && !matches!(source, "pontia" | "pontiad") {
        return Some("Client assembly belongs to the CLI and service entry points");
    }

    if (source.starts_with("pontia-client-")
        || matches!(
            source,
            "pontia-core"
                | "pontia-config"
                | "pontia-runtime"
                | "pontia-storage-sqlite"
                | "pontia-agent-control"
                | "pontia-application"
        ))
        && matches!(
            target,
            "pontia-workflow" | "pontia-bot" | "pontia-http" | "pontia" | "pontiad"
        )
    {
        return Some(
            "Agent Control and its supporting layers are independent of control-plane applications and entry points",
        );
    }

    if matches!(source, "pontia-workflow" | "pontia-bot")
        && matches!(
            target,
            "pontia-storage-sqlite" | "pontia-application" | "pontia-http" | "pontia" | "pontiad"
        )
    {
        return Some(
            "Control-plane applications access Agent Control through its public contracts and own their storage",
        );
    }

    if source == "pontia"
        && matches!(
            target,
            "sqlx"
                | "pontia-storage-sqlite"
                | "pontia-workflow"
                | "pontia-application"
                | "pontia-http"
        )
    {
        return Some("CLI business operations use the service External API");
    }

    if source == "pontia-edge"
        && target.starts_with("pontia")
        && !matches!(target, "pontia-tunnel" | "pontia-update" | "pontia-version")
    {
        return Some(
            "Edge owns connection admission and encrypted forwarding, with shared transport and release utilities",
        );
    }

    None
}

#[test]
fn workspace_production_dependencies_respect_architecture() {
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
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
        .output()
        .expect("read Cargo dependency metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Metadata = serde_json::from_slice(&output.stdout).expect("valid Cargo metadata");
    let mut offenders = Vec::new();
    for package in &metadata.packages {
        if !metadata.workspace_members.contains(&package.id) {
            continue;
        }
        // Metadata includes optional and target-specific declarations on every platform.
        // Use package names so renamed dependencies obey the same boundaries.
        for dependency in &package.dependencies {
            if dependency.kind.as_deref() == Some("dev") {
                continue;
            }
            if let Some(reason) = forbidden_dependency(&package.name, &dependency.name) {
                offenders.push(format!("{} -> {}: {reason}", package.name, dependency.name));
            }
        }
    }
    offenders.sort();
    offenders.dedup();

    assert!(
        offenders.is_empty(),
        "Workspace dependency violations:\n{}",
        offenders.join("\n")
    );
}
