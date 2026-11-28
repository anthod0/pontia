use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use sqlx::SqlitePool;
use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

pub const MAX_FILE_SIZE: u64 = 1 << 30;

#[derive(Debug, thiserror::Error)]
pub enum FileContentError {
    #[error("File no longer exists")]
    NotFound,
    #[error("Access to this file is forbidden")]
    Forbidden,
    #[error("File path has changed")]
    PathChanged,
    #[error("File exceeds the 1 GiB download limit")]
    TooLarge,
    #[error(transparent)]
    Internal(#[from] Error),
}

pub struct FileContent {
    pub file: File,
    pub size: u64,
    pub media_type: &'static str,
    pub name: String,
}

#[derive(Clone)]
pub struct SessionFileService {
    sessions: SqliteSessionRepository,
    pontia_home: PathBuf,
}

impl SessionFileService {
    pub fn new(pool: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            sessions: SqliteSessionRepository::new(pool),
            pontia_home,
        }
    }

    async fn ensure_session(&self, session_id: &str) -> Result<()> {
        if !self.sessions.exists(session_id).await? {
            return Err(Error::NotFound(format!("Session {session_id} not found")));
        }
        Ok(())
    }

    pub async fn path_content(
        &self,
        session_id: &str,
        path: PathBuf,
    ) -> std::result::Result<FileContent, FileContentError> {
        self.ensure_session(session_id).await?;
        if !path.is_absolute() {
            return Err(Error::Domain("An absolute file path is required".into()).into());
        }
        let pontia_home = self.pontia_home.clone();
        tokio::task::spawn_blocking(move || {
            read_content(&path, &pontia_home, std::env::home_dir().as_deref())
        })
        .await
        .map_err(|e| FileContentError::Internal(Error::Io(std::io::Error::other(e))))?
    }
}

fn sensitive_path(path: &Path, pontia_home: &Path, home: Option<&Path>) -> bool {
    let matches = |entry: &Path| {
        path.starts_with(entry)
            || entry
                .canonicalize()
                .is_ok_and(|resolved| path.starts_with(resolved))
    };
    matches(pontia_home)
        || [
            "/etc/shadow",
            "/etc/gshadow",
            "/etc/sudoers",
            "/proc",
            "/sys",
        ]
        .iter()
        .any(|entry| matches(Path::new(entry)))
        || home.is_some_and(|home| {
            [
                ".ssh",
                ".gnupg",
                ".aws",
                ".kube",
                ".docker",
                ".azure",
                ".config/gcloud",
                ".config/gh",
                ".npmrc",
                ".netrc",
                ".pypirc",
                ".git-credentials",
                "Library/Keychains",
            ]
            .iter()
            .any(|entry| matches(&home.join(entry)))
        })
}

fn read_content(
    path: &Path,
    pontia_home: &Path,
    home: Option<&Path>,
) -> std::result::Result<FileContent, FileContentError> {
    let resolved = path.canonicalize().map_err(|error| match error.kind() {
        std::io::ErrorKind::PermissionDenied => FileContentError::Forbidden,
        _ => content_io_error(error),
    })?;
    if sensitive_path(&resolved, pontia_home, home) {
        return Err(FileContentError::Forbidden);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FileContentError::NotFound)?
        .to_owned();
    let (file, media_type) = open_file(&resolved).map_err(content_io_error)?;
    let size = file.metadata().map_err(content_io_error)?.len();
    if size > MAX_FILE_SIZE {
        return Err(FileContentError::TooLarge);
    }
    Ok(FileContent {
        file,
        size,
        media_type,
        name,
    })
}

fn content_io_error(error: std::io::Error) -> FileContentError {
    if matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::LOOP.raw_os_error() || code == rustix::io::Errno::NOTDIR.raw_os_error())
    {
        return FileContentError::PathChanged;
    }
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput => {
            FileContentError::NotFound
        }
        _ => FileContentError::Internal(error.into()),
    }
}

fn open_file(path: &Path) -> std::io::Result<(File, &'static str)> {
    use rustix::fs::{Mode, OFlags, open, openat};
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = open("/", directory_flags, Mode::empty())?;
    for component in path.parent().unwrap_or(path).components() {
        if let std::path::Component::Normal(name) = component {
            directory = openat(&directory, name, directory_flags, Mode::empty())?;
        }
    }
    let mut file = File::from(openat(
        &directory,
        path.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path is not a file")
        })?,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Path is not a regular file",
        ));
    }
    let mut head = [0; 16];
    let mut read = 0;
    while read < head.len() {
        let count = file.read(&mut head[read..])?;
        if count == 0 {
            break;
        }
        read += count;
    }
    file.rewind()?;
    let head = &head[..read];
    let media_type = if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if head.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        "image/gif"
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "application/octet-stream"
    };
    Ok((file, media_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn protects_credentials_and_keeps_preview_paths_readable() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("user");
        let runtime = home.join(".pontia");
        for relative in [
            ".pontia/state/runtime.log",
            ".ssh/id_ed25519",
            ".gnupg/key",
            ".aws/config",
            ".kube/config",
            ".docker/config.json",
            ".azure/token",
            ".config/gcloud/token",
            ".config/gh/hosts.yml",
            "Library/Keychains/login.keychain-db",
            ".npmrc",
            ".netrc",
            ".pypirc",
            ".git-credentials",
        ] {
            let path = home.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"secret").unwrap();
            assert!(
                matches!(
                    read_content(&path, &runtime, Some(&home)),
                    Err(FileContentError::Forbidden)
                ),
                "{relative}"
            );
        }
        for path in [
            home.join("Pontia/report.md"),
            home.join(".ssh-backup/report"),
            root.path().join("ordinary"),
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"preview").unwrap();
            let mut preview = read_content(&path, &runtime, Some(&home)).unwrap();
            let mut bytes = Vec::new();
            preview.file.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"preview");
        }
    }

    #[test]
    fn protects_system_paths() {
        let runtime = tempfile::tempdir().unwrap();
        for path in [
            "/etc/shadow",
            "/etc/gshadow",
            "/etc/sudoers",
            "/proc/cpuinfo",
            "/sys/kernel/uevent_seqnum",
        ] {
            assert!(
                sensitive_path(Path::new(path), runtime.path(), None),
                "{path}"
            );
        }
    }

    #[test]
    fn checks_resolved_paths_and_symlinked_sensitive_directories() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let runtime = home.join(".pontia");
        let keys = home.join("keys");
        fs::create_dir(&keys).unwrap();
        fs::write(keys.join("key"), b"secret").unwrap();
        symlink(&keys, home.join(".ssh")).unwrap();
        symlink(home.join(".ssh"), home.join("shortcut")).unwrap();
        fs::create_dir(home.join("project")).unwrap();
        for path in [
            home.join("shortcut/key"),
            home.join("project/../.ssh/key"),
            keys.join("key"),
        ] {
            assert!(
                matches!(
                    read_content(&path, &runtime, Some(home)),
                    Err(FileContentError::Forbidden)
                ),
                "{}",
                path.display()
            );
        }
    }
}
