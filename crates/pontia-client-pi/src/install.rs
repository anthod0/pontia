use crate::config::PiConfiguration;
use pontia_agent_control::client_contract::{
    DetectionContext,
    install::{
        AuthGuidance, AuthStatus, ClientInstaller, InstallMethod, InstallProvenance, InstallSpec,
        PrerequisiteStatus,
    },
    version,
};
use pontia_runtime::client_operations::{InstallOperation, PackageManager, Phase};
use std::{
    io,
    path::{Path, PathBuf},
};

const PACKAGE: &str = "@earendil-works/pi-coding-agent";
const BINARY: &str = "pi";

fn cli_version(binary: &Path, path: &str, prefix: &str) -> Option<String> {
    // Inspection is synchronous and may be called from an async runtime.
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()?;
                runtime.block_on(async {
                    version::probe(
                        tokio::process::Command::new(binary)
                            .env_clear()
                            .env("PATH", path),
                        prefix,
                    )
                    .await
                })
            })
            .join()
            .expect("version probe panicked")
    })
}

fn prerequisite(context: &DetectionContext<'_>, program: &str) -> PrerequisiteStatus {
    // The package recipe inherits PATH without Pontia's fallback directories.
    let path = context.vars.get("PATH").map(String::as_str);
    let executable = pontia_runtime::executable::find(program, path)
        .ok()
        .flatten();
    let (label, detail, satisfied) = if program == "node" {
        let version = executable
            .as_ref()
            .and_then(|node| cli_version(node, path?, "v"));
        let sufficient = version
            .as_deref()
            .and_then(|v| semver::Version::parse(v).ok())
            .is_some_and(|v| v >= semver::Version::new(22, 19, 0));
        ("Node.js >= 22.19", version, sufficient)
    } else {
        (
            program,
            executable.as_ref().map(|p| p.display().to_string()),
            executable.is_some(),
        )
    };
    PrerequisiteStatus {
        id: program.into(),
        label: label.into(),
        required: program != "tmux",
        satisfied,
        detected_detail: detail,
        missing_hint: (!satisfied)
            .then(|| format!("Install {label} and make it available on PATH.")),
    }
}

fn provenance(context: &DetectionContext<'_>) -> InstallProvenance {
    let path = match context.executable(BINARY) {
        Ok(Some(path)) => path,
        Ok(None) => return InstallProvenance::NotInstalled,
        Err(_) => return InstallProvenance::ExternalUnmanaged,
    };
    let Ok(real) = path.canonicalize() else {
        return InstallProvenance::ExternalUnmanaged;
    };
    if context
        .pontia_home
        .join("tools/pi")
        .canonicalize()
        .is_ok_and(|root| real.starts_with(root))
    {
        InstallProvenance::PontiaManaged
    } else if real.starts_with("/opt/homebrew")
        || real.starts_with("/usr/local/Cellar")
        || real.starts_with("/usr/local/Caskroom")
    {
        InstallProvenance::Homebrew
    } else if real
        .ancestors()
        .any(|p| p.ends_with("lib/node_modules/@earendil-works/pi-coding-agent"))
    {
        InstallProvenance::NpmGlobal
    } else if context
        .vars
        .get("HOME")
        .is_some_and(|home| real.starts_with(Path::new(home).join(".local/bin")))
    {
        InstallProvenance::VendorNative
    } else {
        InstallProvenance::ExternalUnmanaged
    }
}

impl ClientInstaller for PiConfiguration {
    fn client_type(&self) -> &'static str {
        BINARY
    }
    fn client_key(&self) -> &'static str {
        "default"
    }
    fn inspect(&self, context: &DetectionContext<'_>) -> InstallSpec {
        let node = prerequisite(context, "node");
        let tmux = prerequisite(context, "tmux");
        let methods = [("npm_managed", "npm"), ("bun_managed", "bun")]
            .into_iter()
            .map(|(id, manager)| InstallMethod {
                id: id.into(),
                label: format!("Managed {manager} install"),
                recommended: manager == "npm",
                supported_on_platform: cfg!(any(target_os = "macos", target_os = "linux")),
                source_summary: format!("npm: {PACKAGE}"),
                target_summary: context.pontia_home.join("tools/pi").display().to_string(),
                prerequisites: vec![node.clone(), prerequisite(context, manager), tmux.clone()],
            })
            .collect();
        let keys = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"];
        let directory = context
            .vars
            .get("PI_CODING_AGENT_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                context
                    .vars
                    .get("HOME")
                    .map(|h| Path::new(h).join(".pi/agent"))
            });
        let configured = keys
            .iter()
            .any(|key| context.vars.get(*key).is_some_and(|s| !s.is_empty()))
            || directory
                .filter(|d| d.is_absolute())
                .and_then(|d| d.join("auth.json").metadata().ok())
                .is_some_and(|m| m.is_file() && m.len() > 0);
        let provenance = provenance(context);
        let login_binary = match provenance {
            InstallProvenance::NotInstalled | InstallProvenance::PontiaManaged => {
                let path = context.pontia_home.join("tools/bin/pi");
                format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
            }
            _ => BINARY.to_owned(),
        };
        InstallSpec {
            provenance,
            methods,
            auth: AuthGuidance {
                status: if configured {
                    AuthStatus::Configured
                } else {
                    AuthStatus::Unknown
                },
                login_command: Some(login_binary),
                headless_login_command: None,
                env_vars: keys.into_iter().map(String::from).collect(),
                docs_url: Some("https://pi.dev/docs/latest/quickstart".into()),
            },
        }
    }

    fn install(&self, method_id: &str, operation: &mut InstallOperation) -> io::Result<()> {
        self.install_with(
            method_id,
            operation,
            &std::env::vars().collect(),
            |manager, op| op.install_package(manager, BINARY, PACKAGE, BINARY, true),
        )
    }
}

impl PiConfiguration {
    fn install_with(
        &self,
        method_id: &str,
        operation: &mut InstallOperation,
        vars: &std::collections::HashMap<String, String>,
        install_package: impl FnOnce(PackageManager, &InstallOperation) -> io::Result<()>,
    ) -> io::Result<()> {
        let manager = match method_id {
            "npm_managed" => PackageManager::Npm,
            "bun_managed" => PackageManager::Bun,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unknown Pi install method",
                ));
            }
        };
        let spec = self.inspect(&DetectionContext {
            vars,
            pontia_home: operation.pontia_home(),
        });
        if spec.provenance != InstallProvenance::NotInstalled {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Pi is already installed or externally configured",
            ));
        }
        let method = spec
            .methods
            .iter()
            .find(|m| m.id == method_id)
            .expect("registered install method");
        if !method.supported_on_platform {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unsupported Pi platform",
            ));
        }
        for p in &method.prerequisites {
            if let Some(hint) = &p.missing_hint {
                if p.required {
                    return Err(io::Error::new(io::ErrorKind::NotFound, hint.clone()));
                }
                operation.log(hint)?;
            }
        }
        operation.advance(Phase::Installing)?;
        install_package(manager, operation)?;
        operation.advance(Phase::Verifying)?;
        let binary = operation.pontia_home().join("tools/bin/pi");
        let version = cli_version(
            &binary,
            vars.get("PATH").map(String::as_str).unwrap_or_default(),
            "",
        )
        .ok_or_else(|| io::Error::other("Installed Pi did not report a version"))?;
        operation.log(&format!("Installed Pi {version}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pontia_runtime::client_operations::{self, Status};
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn verifies_the_published_command_before_reporting_success() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let node = bin.join("node");
        fs::write(&node, "#!/bin/sh\necho v22.19.0").unwrap();
        fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
        symlink("/bin/true", bin.join("npm")).unwrap();
        let vars = HashMap::from([("PATH".into(), bin.display().to_string())]);
        let installer = PiConfiguration;
        for (version, expected) in [("invalid", Status::Failed), ("1.2.3", Status::Succeeded)] {
            let home = root.path().join(version);
            let state = client_operations::run(&home, BINARY, "default", "npm_managed", |op| {
                installer.install_with("npm_managed", op, &vars, |manager, _| {
                    assert!(matches!(manager, PackageManager::Npm));
                    let bin = home.join("tools/bin");
                    fs::create_dir_all(&bin)?;
                    let command = bin.join(BINARY);
                    fs::write(
                        &command,
                        format!("#!/bin/sh\n[ \"$1\" = --version ] && echo '{version}'"),
                    )?;
                    fs::set_permissions(&command, fs::Permissions::from_mode(0o700))
                })
            })
            .unwrap();
            assert_eq!(state.status, expected, "{:?}", state.error);
        }
    }
}
