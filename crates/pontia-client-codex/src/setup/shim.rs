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

pub(super) struct ShimSetup {
    root: PathBuf,
    shells: Result<ShellFiles, String>,
    pub enabled: bool,
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
        let launcher = root.join("bin/codex");
        let enabled = launcher.is_file() && is_shim(&launcher)?;
        Ok(Self {
            root,
            shells: shell_files(vars, user_home),
            enabled,
        })
    }

    pub(super) fn summary(&self) -> String {
        if self.enabled {
            format!("Codex launcher: {}", self.root.join("bin/codex").display())
        } else {
            "Codex launcher: disabled".into()
        }
    }

    pub(super) fn activation_command(&self) -> String {
        let shells = self
            .shells
            .as_ref()
            .expect("installed launcher has shell files");
        let path = &shells.paths[0];
        let command = if path.file_name().unwrap() == ".profile" {
            "."
        } else {
            "source"
        };
        format!("{command} {}", quote(path.to_str().unwrap()))
    }

    pub(super) fn preflight(&self) -> Result<(), String> {
        let launcher = self.root.join("bin/codex");
        if self.enabled && launcher.try_exists().map_err(|e| e.to_string())? && !is_shim(&launcher)?
        {
            return Err(format!(
                "Refusing to replace an unmanaged executable: {}",
                launcher.display()
            ));
        }
        if self.enabled {
            self.shells.as_ref().map_err(Clone::clone)?;
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
                edit_shell(&path, None)?;
            }
        }
        if !self.enabled {
            if launcher.is_file() && is_shim(&launcher)? {
                fs::remove_file(&launcher).map_err(|e| e.to_string())?;
            }
            if manifest.exists() {
                fs::remove_file(&manifest).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }
        let shells = self.shells.as_ref().map_err(Clone::clone)?;
        let bin = self.root.join("bin");
        fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        let remote = format!("unix://{}", crate::proxy::socket_path(&self.root).display());
        let script = include_str!("shim.sh").replace("__PONTIA_REMOTE__", &quote(&remote));
        fs::write(&launcher, script).map_err(|e| e.to_string())?;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let bin = quote(absolute_utf8_path(&bin, "launcher directory")?);
        let executable = quote(absolute_utf8_path(&launcher, "launcher")?);
        let block = if shells.fish {
            format!(
                "{BEGIN}\nif test -x {executable}\n    set -l pontia_codex_old_path $PATH\n    set -gx PATH {bin}\n    for directory in $pontia_codex_old_path\n        if test \"$directory\" != {bin}\n            set -a PATH \"$directory\"\n        end\n    end\nend\n{END}\n"
            )
        } else {
            format!(
                "{BEGIN}\nif [ -x {executable} ]; then\n    _pontia_codex_rest=$PATH\n    _pontia_codex_path={bin}\n    while :; do\n        _pontia_codex_dir=${{_pontia_codex_rest%%:*}}\n        if [ \"$_pontia_codex_dir\" != {bin} ]; then\n            _pontia_codex_path=$_pontia_codex_path:$_pontia_codex_dir\n        fi\n        case $_pontia_codex_rest in\n            *:*) _pontia_codex_rest=${{_pontia_codex_rest#*:}} ;;\n            *) break ;;\n        esac\n    done\n    export PATH=$_pontia_codex_path\n    unset _pontia_codex_rest _pontia_codex_path _pontia_codex_dir\nfi\n{END}\n"
            )
        };
        for path in &shells.paths {
            edit_shell(path, Some(&block))?;
        }
        fs::create_dir_all(manifest.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(
            &manifest,
            serde_json::to_vec(shells).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
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
