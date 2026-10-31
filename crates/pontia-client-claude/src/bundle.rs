use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/claude-host.tar.gz"));
static DIGEST: OnceLock<String> = OnceLock::new();

pub(crate) fn unpack(root: &Path) -> Result<PathBuf, String> {
    if BUNDLE.is_empty() {
        return Err("This development build has no Claude Agent host; build with scripts/build-claude-host.sh or set PONTIA_CLAUDE_HOST_PATH".into());
    }
    tar::Archive::new(GzDecoder::new(BUNDLE))
        .unpack(root)
        .map_err(|e| e.to_string())?;
    let script = root.join("dist/src/main.js");
    if !script.is_file() {
        return Err("Claude Agent host bundle is incomplete".into());
    }
    Ok(script)
}

pub(crate) fn install(root: &Path) -> Result<PathBuf, String> {
    let digest = DIGEST.get_or_init(|| format!("{:x}", Sha256::digest(BUNDLE)));
    let parent = root.join("clients/claude");
    let destination = parent.join(digest);
    let script = destination.join("dist/src/main.js");
    if script.is_file() {
        return Ok(script);
    }
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let stage = tempfile::tempdir_in(&parent).map_err(|e| e.to_string())?;
    unpack(stage.path())?;
    // Concurrent Session starts may install the same immutable bundle.
    if let Err(error) = fs::rename(stage.path(), &destination)
        && !script.is_file()
    {
        return Err(error.to_string());
    }
    Ok(script)
}
