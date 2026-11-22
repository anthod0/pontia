use pontia_core::{Error, Result};
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
    pool: SqlitePool,
}

impl SessionFileService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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

    pub async fn path_content(
        &self,
        session_id: &str,
        path: PathBuf,
    ) -> std::result::Result<FileContent, FileContentError> {
        self.ensure_session(session_id).await?;
        if !path.is_absolute() {
            return Err(Error::Domain("An absolute file path is required".into()).into());
        }
        tokio::task::spawn_blocking(move || {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(FileContentError::NotFound)?
                .to_owned();
            let path = path.canonicalize().map_err(content_io_error)?;
            read_content(&path, name)
        })
        .await
        .map_err(|e| FileContentError::Internal(Error::Io(std::io::Error::other(e))))?
    }
}

fn read_content(path: &Path, name: String) -> std::result::Result<FileContent, FileContentError> {
    let (file, media_type) = open_file(path).map_err(content_io_error)?;
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
