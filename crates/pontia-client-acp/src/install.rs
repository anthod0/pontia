use pontia_agent_control::client_contract::{DetectionContext, install::*, version};
use pontia_runtime::client_operations::{InstallOperation, Phase, ScriptSource};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

struct Installer {
    key: &'static str,
    binary: &'static str,
    target: &'static str,
    version_prefix: &'static str,
    source: ScriptSource,
    login: &'static str,
    headless: &'static str,
    docs: &'static str,
}

pub fn installers() -> impl Iterator<Item = Arc<dyn ClientInstaller>> {
    builtin()
        .into_iter()
        .map(|installer| Arc::new(installer) as Arc<dyn ClientInstaller>)
}

fn builtin() -> [Installer; 3] {
    [
        Installer {
            key: "antigravity",
            binary: "agy",
            target: ".local/bin/agy",
            version_prefix: "",
            source: ScriptSource {
                url: "https://antigravity.google/cli/install.sh",
                allowed_hosts: &["antigravity.google"],
            },
            login: "agy",
            headless: "agy",
            docs: "https://antigravity.google/docs/cli-install",
        },
        Installer {
            key: "grok",
            binary: "grok",
            target: ".grok/bin/grok",
            version_prefix: "grok ",
            source: ScriptSource {
                url: "https://x.ai/cli/install.sh",
                allowed_hosts: &["x.ai"],
            },
            login: "grok login",
            headless: "grok login --device-auth",
            docs: "https://docs.x.ai/build/overview",
        },
        Installer {
            key: "kiro",
            binary: "kiro-cli",
            target: ".local/bin/kiro-cli",
            version_prefix: "kiro-cli ",
            source: ScriptSource {
                url: "https://cli.kiro.dev/install",
                allowed_hosts: &["cli.kiro.dev"],
            },
            login: "kiro-cli login",
            headless: "kiro-cli login --use-device-flow",
            docs: "https://kiro.dev/docs/getting-started/authentication/",
        },
    ]
}

impl Installer {
    fn provenance(&self, context: &DetectionContext<'_>) -> InstallProvenance {
        let Some(home) = context
            .vars
            .get("HOME")
            .map(Path::new)
            .filter(|home| home.is_absolute())
        else {
            return InstallProvenance::ExternalUnmanaged;
        };
        let target = home.join(self.target);
        let path = match context.executable(self.binary) {
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
        } else if real == home.join(self.target)
            || (self.key == "grok" && real.starts_with(home.join(".grok/downloads")))
            || (self.key == "kiro" && real.starts_with("/Applications/Kiro CLI.app"))
        {
            InstallProvenance::VendorNative
        } else if real.starts_with(context.pontia_home.join("tools")) {
            InstallProvenance::PontiaManaged
        } else if self.key == "grok"
            && real
                .ancestors()
                .any(|dir| dir.ends_with("lib/node_modules/@xai-official/grok"))
        {
            InstallProvenance::NpmGlobal
        } else {
            InstallProvenance::ExternalUnmanaged
        }
    }
}

impl ClientInstaller for Installer {
    fn client_type(&self) -> &'static str {
        "acp"
    }
    fn client_key(&self) -> &'static str {
        self.key
    }

    fn inspect(&self, context: &DetectionContext<'_>) -> InstallSpec {
        let mut prerequisites = vec![
            prerequisite(context, "bash", &["/bin/bash"]),
            prerequisite(context, "curl or wget", &["curl", "wget"]),
        ];
        if self.key == "kiro" {
            prerequisites.push(prerequisite(context, "unzip", &["unzip"]));
            let checksum = if cfg!(target_os = "macos") {
                "shasum"
            } else {
                "sha256sum"
            };
            prerequisites.push(prerequisite(context, checksum, &[checksum]));
        }
        let auth_var = match self.key {
            "grok" => Some("XAI_API_KEY"),
            "kiro" => Some("KIRO_API_KEY"),
            _ => None,
        };
        let configured = auth_var
            .and_then(|key| context.vars.get(key))
            .is_some_and(|v| !v.is_empty())
            || (self.key == "grok"
                && context
                    .vars
                    .get("GROK_HOME")
                    .map(PathBuf::from)
                    .or_else(|| context.vars.get("HOME").map(|h| Path::new(h).join(".grok")))
                    .is_some_and(|h| {
                        h.join("auth.json")
                            .metadata()
                            .is_ok_and(|m| m.is_file() && m.len() > 0)
                    }));
        InstallSpec {
            provenance: self.provenance(context),
            methods: vec![InstallMethod {
                id: "official_script".into(),
                label: "Official installer script".into(),
                recommended: true,
                supported_on_platform: cfg!(all(
                    any(target_os = "linux", target_os = "macos"),
                    any(target_arch = "x86_64", target_arch = "aarch64")
                )),
                source_summary: self.source.url.into(),
                target_summary: format!("~/{}", self.target),
                prerequisites,
            }],
            auth: AuthGuidance {
                status: if configured {
                    AuthStatus::Configured
                } else {
                    AuthStatus::Unknown
                },
                login_command: Some(self.login.into()),
                headless_login_command: Some(self.headless.into()),
                env_vars: auth_var.into_iter().map(String::from).collect(),
                docs_url: Some(self.docs.into()),
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

impl Installer {
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
        if self.key == "kiro"
            && (spec.provenance == InstallProvenance::VendorNative
                || (cfg!(target_os = "macos") && Path::new("/Applications/Kiro CLI.app").exists()))
        {
            return Err(io::Error::other(
                "Use kiro-cli update to update the existing Kiro installation",
            ));
        }
        operation.advance(Phase::Installing)?;
        run_script(operation, &self.source)?;
        operation.advance(Phase::Verifying)?;
        let context = DetectionContext {
            vars,
            pontia_home: operation.pontia_home(),
        };
        let home = Path::new(&vars["HOME"]);
        let discovered = context.executable(self.binary).map_err(io::Error::other)?;
        let publish_grok = self.key == "grok" && discovered.is_none();
        if self.provenance(&context) != InstallProvenance::VendorNative {
            return Err(io::Error::other(format!(
                "Missing vendor installation of {}",
                self.binary
            )));
        }
        let path = discovered.unwrap_or_else(|| home.join(self.target));
        let mut command = tokio::process::Command::new(path);
        command.env_clear().envs(vars);
        let version = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(version::probe(&mut command, self.version_prefix))
            .ok_or_else(|| {
                io::Error::other(format!("Could not verify {} --version", self.binary))
            })?;
        if publish_grok {
            let bin = operation.pontia_home().join("tools/bin");
            std::fs::create_dir_all(&bin)?;
            std::os::unix::fs::symlink(home.join(self.target), bin.join(self.binary))?;
        }
        operation.log(&format!("Installed {} {version}", self.binary))
    }
}

fn prerequisite(
    context: &DetectionContext<'_>,
    label: &str,
    commands: &[&str],
) -> PrerequisiteStatus {
    let path = commands.iter().find_map(|cmd| {
        pontia_runtime::executable::find(cmd, context.vars.get("PATH").map(String::as_str))
            .ok()
            .flatten()
    });
    PrerequisiteStatus {
        id: commands[0].trim_start_matches("/bin/").into(),
        label: label.into(),
        required: true,
        satisfied: path.is_some(),
        missing_hint: path.is_none().then(|| format!("Install {label}")),
        detected_detail: path.map(|p| p.display().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pontia_runtime::client_operations::{Status, run};
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn installs_and_verifies_each_vendor_binary_including_grok_outside_path() {
        for (installer, (target, output)) in builtin().iter().zip([
            (".local/bin/agy", "1.5.15"),
            (".grok/bin/grok", "grok 1.0.92"),
            (".local/bin/kiro-cli", "kiro-cli 3.5.0"),
        ]) {
            let root = tempfile::tempdir().unwrap();
            let pontia = root.path().join("pontia");
            let bin = root.path().join("bin");
            fs::create_dir(&bin).unwrap();
            for tool in ["curl", "unzip", "shasum", "sha256sum"] {
                symlink("/bin/true", bin.join(tool)).unwrap();
            }
            let vars = HashMap::from([
                ("HOME".into(), root.path().display().to_string()),
                ("PATH".into(), bin.display().to_string()),
            ]);
            let state = run(&pontia, "acp", installer.key, "official_script", |op| {
                installer.install_with("official_script", op, &vars, |_, _| {
                    let binary = root.path().join(target);
                    fs::create_dir_all(binary.parent().unwrap())?;
                    crate::test_executable::write_script(
                        &binary,
                        &format!("[ \"$1\" = --version ] && echo '{output}'"),
                    )
                })
            })
            .unwrap();
            assert_eq!(state.status, Status::Succeeded, "{:?}", state.error);
            let context = DetectionContext {
                vars: &vars,
                pontia_home: &pontia,
            };
            let found = context.executable(installer.binary).unwrap().unwrap();
            assert_eq!(found.canonicalize().unwrap(), root.path().join(target));
            symlink("/bin/true", bin.join(installer.binary)).unwrap();
            assert_eq!(
                installer.inspect(&context).provenance,
                InstallProvenance::ExternalUnmanaged
            );
            let refused = run(&pontia, "acp", installer.key, "official_script", |op| {
                installer.install_with("official_script", op, &vars, |_, _| {
                    panic!("attempted to replace external install")
                })
            })
            .unwrap();
            assert_eq!(refused.status, Status::Failed);
        }
    }
}
