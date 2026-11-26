use pontia_agent_control::client_contract::{DetectionContext, install::*, version};
use pontia_runtime::client_operations::{InstallOperation, Phase, ScriptSource};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

pub struct ClaudeInstaller;

const SOURCE: ScriptSource = ScriptSource {
    url: "https://claude.ai/install.sh",
    allowed_hosts: &["claude.ai", "downloads.claude.ai"],
};

impl ClaudeInstaller {
    fn provenance(&self, context: &DetectionContext<'_>) -> InstallProvenance {
        let Some(home) = context
            .vars
            .get("HOME")
            .map(Path::new)
            .filter(|home| home.is_absolute())
        else {
            return InstallProvenance::ExternalUnmanaged;
        };
        let target = home.join(".local/bin/claude");
        let path = match context.executable("claude") {
            Ok(Some(path)) => path,
            Ok(None) => target,
            Err(_) => return InstallProvenance::ExternalUnmanaged,
        };
        if path
            .symlink_metadata()
            .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
        {
            return InstallProvenance::NotInstalled;
        }
        let Ok(real) = path.canonicalize() else {
            return InstallProvenance::ExternalUnmanaged;
        };
        if !pontia_runtime::executable::is_executable(&real).unwrap_or(false) {
            return InstallProvenance::ExternalUnmanaged;
        }
        let home = home.canonicalize().unwrap_or_else(|_| home.into());
        if real.starts_with("/opt/homebrew")
            || real.starts_with("/usr/local/Cellar")
            || real.starts_with("/usr/local/Caskroom")
        {
            InstallProvenance::Homebrew
        } else if real == home.join(".local/bin/claude")
            || real.starts_with(home.join(".local/share/claude/versions"))
        {
            InstallProvenance::VendorNative
        } else if real.starts_with(context.pontia_home.join("tools")) {
            InstallProvenance::PontiaManaged
        } else if real
            .ancestors()
            .any(|dir| dir.ends_with("lib/node_modules/@anthropic-ai/claude-code"))
        {
            InstallProvenance::NpmGlobal
        } else {
            InstallProvenance::ExternalUnmanaged
        }
    }
}

impl ClientInstaller for ClaudeInstaller {
    fn client_type(&self) -> &'static str {
        "claude-agent"
    }
    fn client_key(&self) -> &'static str {
        "default"
    }

    fn inspect(&self, context: &DetectionContext<'_>) -> InstallSpec {
        let node = context
            .vars
            .get("PONTIA_CLAUDE_NODE_PATH")
            .map(String::as_str)
            .unwrap_or("node");
        let config = context
            .vars
            .get("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                context
                    .vars
                    .get("HOME")
                    .map(|h| Path::new(h).join(".claude"))
            });
        let configured = context
            .vars
            .get("ANTHROPIC_API_KEY")
            .is_some_and(|v| !v.is_empty())
            || config.is_some_and(|p| {
                p.join(".credentials.json")
                    .metadata()
                    .is_ok_and(|m| m.is_file() && m.len() > 0)
            });
        InstallSpec {
            provenance: self.provenance(context),
            methods: vec![InstallMethod {
                id: "official_script".into(),
                label: "Official native Claude Code CLI installer".into(),
                recommended: true,
                supported_on_platform: cfg!(all(
                    any(target_os = "linux", target_os = "macos"),
                    any(target_arch = "x86_64", target_arch = "aarch64")
                )),
                source_summary: SOURCE.url.into(),
                target_summary: "~/.local/bin/claude".into(),
                prerequisites: vec![
                    prerequisite(context, "bash", "bash", &["/bin/bash"], true),
                    prerequisite(context, "curl", "curl or wget", &["curl", "wget"], true),
                    prerequisite(
                        context,
                        "node",
                        "Node.js for SDK host setup",
                        &[node],
                        false,
                    ),
                    prerequisite(context, "npm", "npm for SDK host setup", &["npm"], false),
                ],
            }],
            auth: AuthGuidance {
                status: if configured {
                    AuthStatus::Configured
                } else {
                    AuthStatus::Unknown
                },
                login_command: Some("claude auth login".into()),
                headless_login_command: None,
                env_vars: vec!["ANTHROPIC_API_KEY".into()],
                docs_url: Some("https://code.claude.com/docs/en/authentication".into()),
            },
        }
    }
    fn install(&self, method_id: &str, operation: &mut InstallOperation) -> io::Result<()> {
        let vars = ["HOME", "PATH"]
            .into_iter()
            .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_owned(), v)))
            .collect();
        self.install_with(
            method_id,
            operation,
            &vars,
            InstallOperation::install_script,
        )
    }
}

impl ClaudeInstaller {
    fn install_with(
        &self,
        method_id: &str,
        operation: &mut InstallOperation,
        vars: &HashMap<String, String>,
        run_script: impl FnOnce(&InstallOperation, &ScriptSource) -> io::Result<()>,
    ) -> io::Result<()> {
        let context = DetectionContext {
            vars,
            pontia_home: operation.pontia_home(),
        };
        let spec = self.inspect(&context);
        let method = spec
            .methods
            .iter()
            .find(|m| m.id == method_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown install method"))?;
        if !method.supported_on_platform {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unsupported installer platform",
            ));
        }
        if !method.ready() {
            return Err(io::Error::other(format!(
                "Install prerequisites unavailable: {}",
                method
                    .prerequisites
                    .iter()
                    .filter(|p| p.required && !p.satisfied)
                    .map(|p| p.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if !matches!(
            spec.provenance,
            InstallProvenance::NotInstalled | InstallProvenance::VendorNative
        ) {
            return Err(io::Error::other(
                "Use the original installer to update this executable",
            ));
        }
        operation.advance(Phase::Installing)?;
        run_script(operation, &SOURCE)?;
        operation.advance(Phase::Verifying)?;
        let context = DetectionContext {
            vars,
            pontia_home: operation.pontia_home(),
        };
        if self.provenance(&context) != InstallProvenance::VendorNative {
            return Err(io::Error::other("Missing native Claude Code installation"));
        }
        let path = context
            .executable("claude")
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other("Cannot find claude after installation"))?;
        let mut command = tokio::process::Command::new(path);
        command.env_clear().envs(vars);
        let version = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(version::probe(&mut command, ""))
            .ok_or_else(|| io::Error::other("Could not verify claude --version"))?;
        operation.log(&format!("Installed Claude Code CLI {version}"))
    }
}

fn prerequisite(
    context: &DetectionContext<'_>,
    id: &str,
    label: &str,
    commands: &[&str],
    required: bool,
) -> PrerequisiteStatus {
    let path = commands.iter().find_map(|cmd| {
        pontia_runtime::executable::find(cmd, context.vars.get("PATH").map(String::as_str))
            .ok()
            .flatten()
    });
    PrerequisiteStatus {
        id: id.into(),
        label: label.into(),
        required,
        satisfied: path.is_some(),
        missing_hint: path.is_none().then(|| format!("Install {label}")),
        detected_detail: path.map(|p| p.display().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pontia_runtime::client_operations::{Status, run};
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn native_install_is_independent_of_sdk_setup_and_verifies_claude_version() {
        let root = tempfile::tempdir().unwrap();
        let pontia = root.path().join("pontia");
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        symlink("/bin/true", bin.join("wget")).unwrap();
        let host = crate::setup::host_path(&pontia);
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(&host, "SDK host").unwrap();
        let vars = HashMap::from([
            ("HOME".into(), root.path().display().to_string()),
            ("PATH".into(), bin.display().to_string()),
        ]);
        let context = DetectionContext {
            vars: &vars,
            pontia_home: &pontia,
        };
        let spec = ClaudeInstaller.inspect(&context);
        assert_eq!(spec.provenance, InstallProvenance::NotInstalled);
        assert!(spec.methods[0].ready());
        let state = run(
            &pontia,
            "claude-agent",
            "default",
            "official_script",
            |op| {
                ClaudeInstaller.install_with("official_script", op, &vars, |_, _| {
                    let executable = root.path().join(".local/share/claude/versions/test");
                    fs::create_dir_all(executable.parent().unwrap())?;
                    fs::write(
                        &executable,
                        "#!/bin/sh\n[ \"$1\" = --version ] && echo '2.1.318 (Claude Code)'",
                    )?;
                    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
                    let local_bin = root.path().join(".local/bin");
                    fs::create_dir_all(&local_bin)?;
                    symlink(executable, local_bin.join("claude"))
                })
            },
        )
        .unwrap();
        assert_eq!(state.status, Status::Succeeded, "{:?}", state.error);
        assert_eq!(
            ClaudeInstaller.inspect(&context).provenance,
            InstallProvenance::VendorNative
        );
        assert_eq!(fs::read_to_string(host).unwrap(), "SDK host");
    }
}
