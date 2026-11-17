use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub fn is_executable(path: &Path) -> Result<bool, String> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && metadata.permissions().mode() & 0o111 != 0),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
}

/// Relative paths need a Workspace, which is not known on the client settings page.
pub fn find(program: &str, search_path: Option<&str>) -> Result<Option<PathBuf>, String> {
    let command = Path::new(program);
    if command.is_absolute() {
        return Ok(is_executable(command)?.then(|| command.to_path_buf()));
    }
    if program.is_empty() || program.contains('/') {
        return Err("A Workspace is required to resolve this command".into());
    }
    let search_path = search_path.ok_or("PATH is unavailable")?;
    let mut relative = false;
    for directory in std::env::split_paths(search_path) {
        if !directory.is_absolute() {
            relative = true;
            continue;
        }
        let candidate = directory.join(command);
        if is_executable(&candidate)? {
            return Ok(Some(candidate));
        }
    }
    if relative {
        return Err("PATH contains Workspace-relative directories".into());
    }
    Ok(None)
}
