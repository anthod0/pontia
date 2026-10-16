//! ACP Registry discovery and local launch resolution. Registry IDs are not Session identities.
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use pontia_config::AcpAgentConfig;
use pontia_core::{Error, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, OnceCell};

pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

#[derive(Clone)]
pub struct Registry {
    url: String,
    client: reqwest::Client,
    entries: Arc<OnceCell<Vec<Entry>>>,
    install: Arc<Mutex<()>>,
}

#[derive(Clone, Deserialize)]
struct Entry {
    id: String,
    name: String,
    distribution: Distribution,
}

#[derive(Clone, Deserialize)]
struct Distribution {
    binary: Option<BTreeMap<String, Binary>>,
    npx: Option<Package>,
    uvx: Option<Package>,
}

#[derive(Clone, Deserialize)]
struct Package {
    package: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

#[derive(Clone, Deserialize, serde::Serialize)]
struct Binary {
    archive: String,
    cmd: String,
    #[serde(default)]
    args: Vec<String>,
    sha256: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

fn failure(error: impl std::fmt::Display) -> Error {
    Error::Domain(format!("ACP Registry: {error}"))
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(REGISTRY_URL)
    }
}

impl Registry {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("valid Registry HTTP client configuration"),
            entries: Arc::new(OnceCell::new()),
            install: Arc::new(Mutex::new(())),
        }
    }

    async fn entry(&self, id: &str) -> Result<Entry> {
        let entries = self
            .entries
            .get_or_try_init(|| async {
                #[derive(Deserialize)]
                struct Document {
                    agents: Vec<Entry>,
                }
                let document = self
                    .client
                    .get(&self.url)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
                    .map_err(failure)?
                    .error_for_status()
                    .map_err(failure)?
                    .json::<Document>()
                    .await
                    .map_err(failure)?;
                Ok::<_, Error>(document.agents)
            })
            .await?;
        entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| failure(format!("unknown Agent: {id}")))
    }

    pub(crate) fn cached_name(&self, id: &str) -> Option<String> {
        self.entries
            .get()?
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.name.clone())
    }

    pub async fn name(&self, id: &str) -> Result<String> {
        Ok(self.entry(id).await?.name)
    }

    pub async fn resolve(
        &self,
        config: &AcpAgentConfig,
        cache: &Path,
        cwd: &Path,
    ) -> Result<AcpAgentConfig> {
        let Some(id) = &config.registry_id else {
            return crate::peer::resolve_command(config, cwd);
        };
        let entry = self.entry(id).await?;
        let mut resolved = config.clone();
        resolved.registry_id = None;
        resolved.name = config.name.clone().or(Some(entry.name));
        let platform = format!(
            "{}-{}",
            if std::env::consts::OS == "macos" {
                "darwin"
            } else {
                std::env::consts::OS
            },
            std::env::consts::ARCH
        );
        if let Some(binary) = entry
            .distribution
            .binary
            .as_ref()
            .and_then(|binary| binary.get(&platform))
        {
            resolved.command = self
                .binary(binary, cache)
                .await?
                .to_str()
                .ok_or_else(|| failure("non UTF-8 executable path"))?
                .into();
            resolved.args = binary.args.clone();
            resolved.env = binary.env.clone();
        } else if let Some(package) = entry.distribution.npx {
            resolved.command = "npx".into();
            resolved.args = vec!["--yes".into(), "--".into(), package.package];
            resolved.args.extend(package.args);
            resolved.env = package.env;
        } else if let Some(package) = entry.distribution.uvx {
            resolved.command = "uvx".into();
            resolved.args = vec!["--".into(), package.package];
            resolved.args.extend(package.args);
            resolved.env = package.env;
        } else {
            return Err(failure(format!(
                "no supported distribution for {id} on {platform}"
            )));
        }
        resolved.env.extend(config.env.clone());
        crate::peer::resolve_command(&resolved, cwd)
    }

    async fn binary(&self, binary: &Binary, cache: &Path) -> Result<PathBuf> {
        let command = Path::new(&binary.cmd);
        if !command
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
            || !command
                .components()
                .any(|part| matches!(part, Component::Normal(_)))
        {
            return Err(failure("binary command must be relative to its archive"));
        }
        let key = format!("{:x}", Sha256::digest(serde_json::to_vec(binary)?));
        let cache = std::path::absolute(cache)?;
        let directory = cache.join(key);
        let executable = directory.join(command);
        let _install = self.install.lock().await;
        if executable.is_file() {
            return Ok(executable);
        }
        let bytes = self
            .client
            .get(&binary.archive)
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .map_err(failure)?
            .error_for_status()
            .map_err(failure)?
            .bytes()
            .await
            .map_err(failure)?;
        if let Some(expected) = &binary.sha256 {
            let actual = format!("{:x}", Sha256::digest(&bytes));
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(failure("archive checksum mismatch"));
            }
        }
        let cache = cache.to_path_buf();
        let binary = binary.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&cache)?;
            let staging = tempfile::tempdir_in(&cache)?;
            extract(&bytes, &binary.archive, staging.path())?;
            let command = staging.path().join(&binary.cmd);
            if !command.is_file() {
                return Err(failure("archive does not contain its declared executable"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = command.metadata()?.permissions();
                permissions.set_mode(permissions.mode() | 0o700);
                std::fs::set_permissions(&command, permissions)?;
            }
            // Publish only a fully extracted and validated installation.
            std::fs::rename(staging.path(), &directory)?;
            Ok(executable)
        })
        .await
        .map_err(failure)?
    }
}

fn extract(bytes: &[u8], url: &str, root: &Path) -> Result<()> {
    if url
        .split('?')
        .next()
        .is_some_and(|url| url.ends_with(".zip"))
    {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(failure)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(failure)?;
            let path = entry
                .enclosed_name()
                .ok_or_else(|| failure("unsafe archive path"))?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(failure("archive links are unsupported"));
            }
            let target = root.join(path);
            if entry.is_dir() {
                std::fs::create_dir_all(target)?;
            } else {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut output = std::fs::File::create(target)?;
                std::io::copy(&mut entry, &mut output)?;
                output.flush()?;
            }
        }
    } else {
        let reader: Box<dyn Read + '_> = if bytes.starts_with(&[0x1f, 0x8b]) {
            Box::new(flate2::read::GzDecoder::new(bytes))
        } else if bytes.starts_with(b"BZh") {
            Box::new(bzip2::read::BzDecoder::new(bytes))
        } else {
            Box::new(bytes)
        };
        for entry in tar::Archive::new(reader).entries()? {
            let mut entry = entry?;
            if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
                return Err(failure("archive links and special files are unsupported"));
            }
            if !entry.unpack_in(root)? {
                return Err(failure("unsafe archive path"));
            }
        }
    }
    Ok(())
}
