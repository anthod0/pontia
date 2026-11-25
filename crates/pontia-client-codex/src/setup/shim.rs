use std::{
    collections::HashMap,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use pontia_runtime::local_service::absolute_utf8_path;
use serde::{Deserialize, Serialize};

const MARKER: &str = "# Pontia Codex launcher";
const BEGIN: &str = "# >>> Pontia Codex PATH >>>";
const END: &str = "# <<< Pontia Codex PATH <<<";

#[derive(Serialize, Deserialize)]
struct ShellFiles {
    paths: Vec<PathBuf>,
    fish: bool,
}

pub(super) struct ShimUninstall {
    root: PathBuf,
    shells: Vec<PathBuf>,
}

impl ShimUninstall {
    pub(super) fn inspect(root: &Path) -> Result<Self, String> {
        let manifest = root.join("state/codex-shim.json");
        let shells: Option<ShellFiles> = read_optional(&manifest)?
            .map(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
            .transpose()?;
        let uninstall = Self {
            root: root.into(),
            shells: shells.map(|shells| shells.paths).unwrap_or_default(),
        };
        uninstall.preflight()?;
        Ok(uninstall)
    }

    fn preflight(&self) -> Result<(), String> {
        let launcher = self.root.join("bin/codex");
        match fs::symlink_metadata(&launcher) {
            Ok(_) if !is_shim(&launcher)? => {
                return Err(format!(
                    "Refusing to remove an unmanaged executable: {}",
                    launcher.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        for path in &self.shells {
            absolute_utf8_path(path, "shell configuration path")?;
            if let Some(text) = read_optional(path)?
                && let Some(start) = text.find(BEGIN)
                && !text[start..].contains(END)
            {
                return Err(format!(
                    "Unterminated Pontia PATH block in {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    pub(super) fn summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.root.join("bin/codex").exists() {
            lines.push(format!(
                "Remove Codex launcher: {}",
                self.root.join("bin/codex").display()
            ));
        }
        for path in &self.shells {
            lines.push(format!("Remove Pontia PATH block from {}", path.display()));
        }
        lines
    }

    pub(super) fn uninstall(&self) -> Result<(), String> {
        self.preflight()?;
        for path in &self.shells {
            edit_shell(path, None)?;
        }
        pontia_runtime::local_service::remove_file_if_present(&self.root.join("bin/codex"))?;
        pontia_runtime::local_service::remove_file_if_present(&self.root.join("env.sh"))?;
        pontia_runtime::local_service::remove_file_if_present(&self.root.join("env.fish"))?;
        pontia_runtime::local_service::remove_file_if_present(
            &self.root.join("state/codex-shim.json"),
        )
    }
}

pub(super) struct ShimSetup {
    root: PathBuf,
    shells: ShellFiles,
}

pub(super) fn is_shim(path: &Path) -> Result<bool, String> {
    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(fs::File::open(path).map_err(|e| e.to_string())?);
    let mut prefix = [0; 10];
    use std::io::Read;
    let count = reader.read(&mut prefix).map_err(|e| e.to_string())?;
    if !prefix[..count].starts_with(b"#!/bin/sh\n") {
        return Ok(false);
    }
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(line.trim_end() == MARKER)
}

pub(super) fn detect_setup(root: &Path) -> Result<super::ClientSetup, String> {
    use super::ClientSetup;
    let launcher = root.join("bin/codex");
    if !pontia_runtime::executable::is_executable(&launcher)? || !is_shim(&launcher)? {
        return Ok(ClientSetup::Required(format!(
            "Missing Pontia launcher at {}",
            launcher.display()
        )));
    }
    let manifest = root.join("state/codex-shim.json");
    let Some(text) = read_optional(&manifest)? else {
        return Ok(ClientSetup::Required(format!(
            "Missing {}",
            manifest.display()
        )));
    };
    let shells: ShellFiles = serde_json::from_str(&text)
        .map_err(|error| format!("Invalid {}: {error}", manifest.display()))?;
    let setup = ShimSetup {
        root: root.into(),
        shells,
    };
    let env_path = setup.env_script_path();
    if read_optional(&env_path)?.is_none_or(|script| script.is_empty()) {
        return Ok(ClientSetup::Required(format!(
            "Missing activation script at {}",
            env_path.display()
        )));
    }
    let block = setup.shell_block()?;
    if setup.shells.paths.is_empty() {
        return Ok(ClientSetup::Required(
            "Missing shell configuration paths".into(),
        ));
    }
    for path in &setup.shells.paths {
        if read_optional(path)?.is_none_or(|text| !text.contains(block.trim_end())) {
            return Ok(ClientSetup::Required(format!(
                "Missing launcher PATH block in {}",
                path.display()
            )));
        }
    }
    Ok(ClientSetup::Initialized)
}

impl ShimSetup {
    pub(super) fn inspect(
        vars: &HashMap<String, String>,
        user_home: &Path,
    ) -> Result<Self, String> {
        let root = vars
            .get("PONTIA_HOME")
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| user_home.join(".pontia"));
        absolute_utf8_path(&root, "PONTIA_HOME")?;
        Ok(Self {
            root,
            shells: shell_files(vars, user_home)?,
        })
    }

    pub(super) fn summary(&self) -> String {
        format!("Launcher: {}", self.root.join("bin/codex").display())
    }

    fn env_script_path(&self) -> PathBuf {
        self.root.join(if self.shells.fish {
            "env.fish"
        } else {
            "env.sh"
        })
    }

    fn shell_block(&self) -> Result<String, String> {
        let env_quoted = quote(absolute_utf8_path(
            &self.env_script_path(),
            "shell environment script",
        )?);
        Ok(if self.shells.fish {
            format!("{BEGIN}\nif test -f {env_quoted}; source {env_quoted}; end\n{END}\n")
        } else {
            format!("{BEGIN}\nif [ -f {env_quoted} ]; then . {env_quoted}; fi\n{END}\n")
        })
    }

    pub(super) fn activation_command(&self) -> String {
        let path = self.env_script_path();
        let command = if self.shells.fish { "source" } else { "." };
        format!("{command} {}", quote(path.to_str().unwrap()))
    }

    pub(super) fn preflight(&self) -> Result<(), String> {
        let launcher = self.root.join("bin/codex");
        if fs::symlink_metadata(&launcher)
            .map(|_| true)
            .or_else(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    Ok(false)
                } else {
                    Err(error)
                }
            })
            .map_err(|e| e.to_string())?
            && !is_shim(&launcher)?
        {
            return Err(format!(
                "Refusing to replace an unmanaged executable: {}",
                launcher.display()
            ));
        }
        Ok(())
    }

    pub(super) fn install(&self) -> Result<(), String> {
        self.preflight()?;
        let launcher = self.root.join("bin/codex");
        let manifest = self.root.join("state/codex-shim.json");
        let previous: Option<ShellFiles> = read_optional(&manifest)?
            .map(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .transpose()?;
        if let Some(previous) = previous {
            for path in previous.paths {
                if !self.shells.paths.contains(&path) {
                    edit_shell(&path, None)?;
                }
            }
        }
        let shells = &self.shells;
        let bin = self.root.join("bin");
        fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        let remote = format!("unix://{}", crate::proxy::socket_path(&self.root).display());
        let script = include_str!("shim.sh")
            .replace("__PONTIA_REMOTE__", &quote(&remote))
            .replace("__PONTIA_HOME__", &quote(self.root.to_str().unwrap()));
        write_changed(&launcher, &script)?;
        if fs::metadata(&launcher)
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777
            != 0o755
        {
            fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let bin = quote(absolute_utf8_path(&bin, "launcher directory")?);
        let executable = quote(absolute_utf8_path(&launcher, "launcher")?);
        let env_path = self.env_script_path();
        let stale_env_path = self
            .root
            .join(if shells.fish { "env.sh" } else { "env.fish" });
        let env_script = if shells.fish {
            format!(
                "if test -x {executable}\n    fish_add_path --path --move --prepend {bin}\nend\n"
            )
        } else {
            format!(
                "if [ -x {executable} ]; then\n    _pontia_bin={bin}\n    _pontia_path=:${{PATH-}}:\n    while :; do\n        case $_pontia_path in\n            *:\"$_pontia_bin\":*) _pontia_path=${{_pontia_path%%:\"$_pontia_bin\":*}}:${{_pontia_path#*:\"$_pontia_bin\":}} ;;\n            *) break ;;\n        esac\n    done\n    _pontia_path=${{_pontia_path#:}}\n    _pontia_path=${{_pontia_path%:}}\n    export PATH=\"$_pontia_bin${{_pontia_path:+:$_pontia_path}}\"\n    unset _pontia_bin _pontia_path\nfi\n"
            )
        };
        write_changed(&env_path, &env_script)?;
        pontia_runtime::local_service::remove_file_if_present(&stale_env_path)?;
        let block = self.shell_block()?;
        for path in &shells.paths {
            edit_shell(path, Some(&block))?;
        }
        fs::create_dir_all(manifest.parent().unwrap()).map_err(|e| e.to_string())?;
        write_changed(
            &manifest,
            &serde_json::to_string(shells).map_err(|e| e.to_string())?,
        )
    }
}

fn shell_files(vars: &HashMap<String, String>, home: &Path) -> Result<ShellFiles, String> {
    let shell = vars
        .get("SHELL")
        .and_then(|s| Path::new(s).file_name())
        .and_then(|s| s.to_str());
    let (paths, fish) = match shell {
        Some("bash") => {
            let login = [".bash_profile", ".bash_login", ".profile"]
                .into_iter()
                .map(|name| home.join(name))
                .find(|path| path.exists())
                .unwrap_or_else(|| home.join(".profile"));
            (vec![home.join(".bashrc"), login], false)
        }
        Some("zsh") => {
            let root = vars
                .get("ZDOTDIR")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.to_path_buf());
            absolute_utf8_path(&root, "ZDOTDIR")?;
            (vec![root.join(".zshrc"), root.join(".zprofile")], false)
        }
        Some("fish") => {
            let root = vars
                .get("XDG_CONFIG_HOME")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            absolute_utf8_path(&root, "XDG_CONFIG_HOME")?;
            (vec![root.join("fish/config.fish")], true)
        }
        Some("sh" | "dash" | "ksh") => (vec![home.join(".profile")], false),
        _ => return Err(
            "Codex launcher PATH setup requires SHELL to select bash, zsh, fish, sh, dash, or ksh"
                .into(),
        ),
    };
    Ok(ShellFiles { paths, fish })
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn write_changed(path: &Path, text: &str) -> Result<(), String> {
    if read_optional(path)?.as_deref() != Some(text) {
        fs::write(path, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn edit_shell(path: &Path, block: Option<&str>) -> Result<(), String> {
    let original = read_optional(path)?.unwrap_or_default();
    let mut text = original.clone();
    if let Some(start) = text.find(BEGIN) {
        let end = text[start..]
            .find(END)
            .ok_or_else(|| format!("Unterminated Pontia PATH block in {}", path.display()))?
            + start
            + END.len();
        let end = end + usize::from(text[end..].starts_with('\n'));
        text.replace_range(start..end, "");
    }
    if let Some(block) = block {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(block);
    }
    if text != original {
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
