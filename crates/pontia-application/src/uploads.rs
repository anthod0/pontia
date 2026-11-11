use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use rustix::fs::{self, FlockOperation, Mode, OFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

mod cleanup;

pub const MAX_UPLOAD_SIZE: u64 = 1 << 30;
pub const MAX_CHUNK_SIZE: usize = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("Invalid upload parameters")]
    Invalid,
    #[error("Upload not found")]
    NotFound,
    #[error("Upload exceeds the size limit")]
    TooLarge,
    #[error("Idempotency key belongs to a different file")]
    IdempotencyConflict,
    #[error("Upload offset has changed; query progress before continuing")]
    OffsetConflict,
    #[error("Upload is incomplete")]
    Incomplete,
    #[error("Upload is already complete")]
    Completed,
    #[error("File checksum does not match; create a new upload")]
    ChecksumMismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Record(#[from] serde_json::Error),
}
impl From<rustix::io::Errno> for UploadError {
    fn from(error: rustix::io::Errno) -> Self {
        Self::Io(error.into())
    }
}
type Result<T> = std::result::Result<T, UploadError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload {
    pub upload_id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub offset: u64,
    pub completed: bool,
    pub created_at: String,
    pub path: Option<String>,
}

#[derive(Clone)]
pub struct UploadService {
    home: PathBuf,
}

impl UploadService {
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    pub fn create(
        &self,
        key: &str,
        name: String,
        size: u64,
        sha256: String,
    ) -> Result<(Upload, bool)> {
        let id = upload_id(key)?;
        if name.is_empty()
            || matches!(name.as_str(), "." | "..")
            || name.contains(['/', '\\', '\0'])
            || sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(UploadError::Invalid);
        }
        if size > MAX_UPLOAD_SIZE {
            return Err(UploadError::TooLarge);
        }
        let root = self.root()?;
        let dir = directory(&root, &id, true)?;
        fs::flock(&dir, FlockOperation::LockExclusive)?;
        // Sync the parent even on a retry following a crash during directory creation.
        root.sync_all()?;
        match load(&dir) {
            Ok(upload) => {
                if upload.name != name || upload.size != size || upload.sha256 != sha256 {
                    return Err(UploadError::IdempotencyConflict);
                }
                return Ok((upload, false));
            }
            Err(UploadError::NotFound) => {}
            Err(error) => return Err(error),
        }
        let files = directory(&dir, "files", true)?;
        let file = open(&files, &name, OFlags::WRONLY | OFlags::CREATE)?;
        file.set_len(0)?;
        file.sync_all()?;
        files.sync_all()?;
        let upload = Upload {
            upload_id: id,
            name,
            size,
            sha256,
            offset: 0,
            completed: false,
            created_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .expect("UTC timestamp"),
            path: None,
        };
        save(&dir, &upload)?;
        Ok((upload, true))
    }

    pub fn get(&self, id: &str) -> Result<Upload> {
        let dir = self.locked(id)?;
        load(&dir)
    }

    pub fn append(&self, id: &str, offset: u64, bytes: &[u8]) -> Result<Upload> {
        if bytes.len() > MAX_CHUNK_SIZE {
            return Err(UploadError::TooLarge);
        }
        if bytes.is_empty() {
            return Err(UploadError::Invalid);
        }
        let dir = self.locked(id)?;
        let mut upload = load(&dir)?;
        if upload.completed {
            return Err(UploadError::Completed);
        }
        if offset != upload.offset {
            return Err(UploadError::OffsetConflict);
        }
        if bytes.len() as u64 > upload.size - offset {
            return Err(UploadError::Invalid);
        }
        let files = directory(&dir, "files", false)?;
        let mut file = open(&files, &upload.name, OFlags::WRONLY)?;
        // Discard unacknowledged bytes left behind by an interrupted write.
        file.set_len(offset)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        upload.offset += bytes.len() as u64;
        save(&dir, &upload)?;
        Ok(upload)
    }

    pub fn complete(&self, id: &str) -> Result<Upload> {
        let dir = self.locked(id)?;
        let mut upload = load(&dir)?;
        if upload.completed {
            return Ok(upload);
        }
        if upload.offset != upload.size {
            return Err(UploadError::Incomplete);
        }
        let files = directory(&dir, "files", false)?;
        let mut file = open(&files, &upload.name, OFlags::RDONLY)?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        if file.metadata()?.len() != upload.size
            || format!("{:x}", digest.finalize()) != upload.sha256
        {
            return Err(UploadError::ChecksumMismatch);
        }
        upload.completed = true;
        upload.path = Some(
            self.home
                .canonicalize()?
                .join("state/uploads")
                .join(&upload.upload_id)
                .join("files")
                .join(&upload.name)
                .to_str()
                .ok_or(UploadError::Invalid)?
                .to_owned(),
        );
        save(&dir, &upload)?;
        Ok(upload)
    }

    fn root(&self) -> Result<File> {
        let home = File::from(fs::open(
            &self.home,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let state = directory(&home, "state", true)?;
        let root = directory(&state, "uploads", true)?;
        home.sync_all()?;
        state.sync_all()?;
        Ok(root)
    }

    fn locked(&self, id: &str) -> Result<File> {
        let id = upload_id(id)?;
        let dir = directory(&self.root()?, &id, false).map_err(|error| match error {
            UploadError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                UploadError::NotFound
            }
            error => error,
        })?;
        fs::flock(&dir, FlockOperation::LockExclusive)?;
        Ok(dir)
    }
}

fn upload_id(value: &str) -> Result<String> {
    let id = Uuid::parse_str(value).map_err(|_| UploadError::Invalid)?;
    if id.get_version_num() != 4
        || id.get_variant() != uuid::Variant::RFC4122
        || !id.hyphenated().to_string().eq_ignore_ascii_case(value)
    {
        return Err(UploadError::Invalid);
    }
    Ok(id.to_string())
}

fn directory(parent: &File, name: &str, create: bool) -> Result<File> {
    if create {
        match fs::mkdirat(parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(error.into()),
        }
    }
    open(parent, name, OFlags::RDONLY | OFlags::DIRECTORY)
}

fn open(parent: &File, name: &str, flags: OFlags) -> Result<File> {
    Ok(File::from(fs::openat(
        parent,
        Path::new(name),
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?))
}

fn load(dir: &File) -> Result<Upload> {
    // Reconcile a previous rename whose directory sync failed before exposing its offset.
    sync_record_directory(dir)?;
    let file = open(dir, "record.json", OFlags::RDONLY).map_err(|error| match error {
        UploadError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => UploadError::NotFound,
        error => error,
    })?;
    Ok(serde_json::from_reader(file)?)
}

fn save(dir: &File, upload: &Upload) -> Result<()> {
    let mut file = open(
        dir,
        "record.next",
        OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC,
    )?;
    serde_json::to_writer(&mut file, upload)?;
    file.sync_all()?;
    fs::renameat(dir, "record.next", dir, "record.json")?;
    sync_record_directory(dir)?;
    Ok(())
}

fn sync_record_directory(dir: &File) -> std::io::Result<()> {
    #[cfg(test)]
    SYNC_FAILURE_AFTER.with(|remaining| {
        if let Some(count) = remaining.get() {
            if count == 0 {
                return Err(std::io::Error::other("injected directory sync failure"));
            }
            remaining.set(Some(count - 1));
        }
        Ok(())
    })?;
    dir.sync_all()
}

#[cfg(test)]
thread_local! {
    static SYNC_FAILURE_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_record_sync_is_reconciled_before_returning_progress() {
        let root = tempfile::tempdir().unwrap();
        let service = UploadService::new(root.path().into());
        let id = "12345678-1234-4234-8234-123456789abc";
        service
            .create(id, "file".into(), 1, format!("{:x}", Sha256::digest(b"a")))
            .unwrap();
        SYNC_FAILURE_AFTER.set(Some(1));
        assert!(matches!(
            service.append(id, 0, b"a"),
            Err(UploadError::Io(_))
        ));
        assert!(matches!(service.get(id), Err(UploadError::Io(_))));
        assert!(matches!(service.complete(id), Err(UploadError::Io(_))));
        SYNC_FAILURE_AFTER.set(None);
        assert_eq!(service.get(id).unwrap().offset, 1);
        assert!(service.complete(id).unwrap().completed);
    }
}
