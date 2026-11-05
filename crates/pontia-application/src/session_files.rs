use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::session_files::SessionFileRepository;
use serde::Serialize;
use sqlx::SqlitePool;
use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub use pontia_storage_sqlite::models::session_files::SharedFile;
pub const MAX_FILE_SIZE: u64 = 1 << 30;

#[derive(Debug, thiserror::Error)]
pub enum FileContentError {
    #[error("File no longer exists")]
    NotFound,
    #[error("File path has changed; ask the Agent to share it again")]
    PathChanged,
    #[error("File exceeds the 1 GiB download limit")]
    TooLarge,
    #[error(transparent)]
    Internal(#[from] Error),
}

pub struct SharedFileContent {
    pub file: File,
    pub size: u64,
    pub media_type: &'static str,
    pub name: String,
}

#[derive(Serialize)]
pub struct SharedFilePage {
    pub files: Vec<SharedFile>,
    pub next_cursor: Option<String>,
}

#[derive(Clone)]
pub struct SessionFileService {
    pool: SqlitePool,
    repository: SessionFileRepository,
}

impl SessionFileService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            repository: SessionFileRepository::new(pool.clone()),
            pool,
        }
    }

    async fn ensure_session(&self, session_id: &str) -> Result<()> {
        if !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id = ?)",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?
        {
            return Err(Error::NotFound(format!("Session {session_id} not found")));
        }
        Ok(())
    }

    pub async fn share(&self, session_id: &str, path: PathBuf) -> Result<SharedFile> {
        self.ensure_session(session_id).await?;
        let file = tokio::task::spawn_blocking(move || {
            if !path.is_absolute() {
                return Err(Error::Domain("An absolute file path is required".into()));
            }
            let path = path
                .canonicalize()
                .map_err(|e| Error::Domain(e.to_string()))?;
            let (file, media_type) = open_file(&path).map_err(|e| Error::Domain(e.to_string()))?;
            let size = file.metadata()?.len();
            if size > MAX_FILE_SIZE {
                return Err(Error::Domain(
                    "File exceeds the 1 GiB download limit".into(),
                ));
            }
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| Error::Domain("File name must be UTF-8".into()))?
                .to_owned();
            let path = path
                .to_str()
                .ok_or_else(|| Error::Domain("File path must be UTF-8".into()))?
                .to_owned();
            Ok(SharedFile {
                file_id: format!("file_{}", Uuid::now_v7()),
                turn_id: None,
                path,
                name,
                size: size as i64,
                media_type: media_type.into(),
                created_at: pontia_core::time::utc_now()
                    .format(&time::format_description::well_known::Rfc3339)
                    .expect("UTC timestamp"),
            })
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e)))??;
        self.repository
            .share(
                session_id,
                file,
                &pontia_core::ids::new_event_id().to_string(),
            )
            .await
    }

    pub async fn list(
        &self,
        session_id: &str,
        limit: Option<i64>,
        cursor: Option<&str>,
    ) -> Result<SharedFilePage> {
        self.ensure_session(session_id).await?;
        let limit = limit.unwrap_or(50);
        if !(1..=200).contains(&limit) {
            return Err(Error::Domain("limit must be between 1 and 200".into()));
        }
        let mut files = self.repository.list(session_id, limit + 1, cursor).await?;
        let next_cursor = if files.len() > limit as usize {
            files.truncate(limit as usize);
            files.last().map(|f| f.file_id.clone())
        } else {
            None
        };
        Ok(SharedFilePage { files, next_cursor })
    }

    pub async fn content(
        &self,
        session_id: &str,
        file_id: &str,
    ) -> std::result::Result<SharedFileContent, FileContentError> {
        let record = self
            .repository
            .get(session_id, file_id)
            .await?
            .ok_or(FileContentError::NotFound)?;
        tokio::task::spawn_blocking(move || {
            let path = Path::new(&record.path);
            let current = path.canonicalize().map_err(content_io_error)?;
            if current != path {
                return Err(FileContentError::PathChanged);
            }
            let (file, media_type) = open_file(path).map_err(content_io_error)?;
            let size = file.metadata().map_err(content_io_error)?.len();
            if size > MAX_FILE_SIZE {
                return Err(FileContentError::TooLarge);
            }
            Ok(SharedFileContent {
                file,
                size,
                media_type,
                name: record.name,
            })
        })
        .await
        .map_err(|e| FileContentError::Internal(Error::Io(std::io::Error::other(e))))?
    }
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
