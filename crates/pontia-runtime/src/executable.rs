use std::{
    ffi::OsStr,
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

pub fn effective_search_path(
    inherited: Option<&OsStr>,
    user_home: Option<&Path>,
    pontia_home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs: Vec<_> = inherited
        .into_iter()
        .flat_map(std::env::split_paths)
        .collect();
    let default_root = user_home.map(|home| home.join(".pontia"));
    for dir in [
        pontia_home
            .or(default_root.as_deref())
            .map(|home| home.join("tools/bin")),
        user_home.map(|home| home.join(".local/bin")),
    ]
    .into_iter()
    .flatten()
    {
        if dir.is_absolute() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

pub fn inherited_search_path(pontia_home: Option<&Path>) -> Vec<PathBuf> {
    let configured = std::env::var_os("PONTIA_HOME").map(PathBuf::from);
    effective_search_path(
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("HOME").as_deref().map(Path::new),
        pontia_home.or(configured.as_deref()),
    )
}

/// Relative paths need a Workspace, which is not known on the client settings page.
pub fn find(program: &str, search_path: Option<&str>) -> Result<Option<PathBuf>, String> {
    if search_path.is_none()
        && !Path::new(program).is_absolute()
        && !program.contains('/')
        && !program.is_empty()
    {
        return Err("PATH is unavailable".into());
    }
    find_matching(
        program,
        search_path.into_iter().flat_map(std::env::split_paths),
        |_| Ok(true),
    )
}

pub fn find_matching(
    program: &str,
    directories: impl IntoIterator<Item = PathBuf>,
    accept: impl Fn(&Path) -> Result<bool, String>,
) -> Result<Option<PathBuf>, String> {
    let command = Path::new(program);
    if command.is_absolute() {
        return Ok((is_executable(command)? && accept(command)?).then(|| command.to_path_buf()));
    }
    if program.is_empty() || program.contains('/') {
        return Err("A Workspace is required to resolve this command".into());
    }

    let mut relative = false;
    for directory in directories {
        if !directory.is_absolute() {
            relative = true;
            continue;
        }
        let candidate = directory.join(command);
        if is_executable(&candidate)? && accept(&candidate)? {
            return Ok(Some(candidate));
        }
    }
    if relative {
        return Err("PATH contains Workspace-relative directories".into());
    }
    Ok(None)
}
