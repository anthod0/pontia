use pontia_config::UploadsConfig;
use rustix::fs::{AtFlags, Dir};
use time::Duration;

use super::*;

impl UploadService {
    pub fn cleanup(&self, config: &UploadsConfig, now: OffsetDateTime) -> Result<usize> {
        let root = self.root()?;
        let mut deleted = 0;
        for entry in Dir::read_from(&root)? {
            let entry = entry?;
            let Ok(id) = entry.file_name().to_str() else {
                continue;
            };
            if !matches!(upload_id(id), Ok(normalized) if normalized == id) {
                continue;
            }
            match cleanup_upload(&root, id, config, now) {
                Ok(true) => deleted += 1,
                Ok(false) | Err(UploadError::NotFound) => {}
                Err(error) => tracing::warn!(upload_id = id, %error, "failed to clean up upload"),
            }
        }
        Ok(deleted)
    }
}

fn cleanup_upload(
    root: &File,
    id: &str,
    config: &UploadsConfig,
    now: OffsetDateTime,
) -> Result<bool> {
    let dir = directory(root, id, false)?;
    match fs::flock(&dir, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {}
        Err(rustix::io::Errno::WOULDBLOCK) => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    let upload = load(&dir)?;
    let created_at =
        OffsetDateTime::parse(&upload.created_at, &Rfc3339).map_err(|_| UploadError::Invalid)?;
    let retention_days = if upload.completed {
        config.completed_retention_days
    } else {
        config.incomplete_retention_days
    };
    if now - created_at < Duration::days(i64::from(retention_days.get())) {
        return Ok(false);
    }

    // Work relative to locked directories; never follow a stored path or a symlink.
    match directory(&dir, "files", false) {
        Ok(files) => {
            for entry in Dir::read_from(&files)? {
                let entry = entry?;
                let name = entry.file_name();
                if name == c"." || name == c".." {
                    continue;
                }
                fs::unlinkat(&files, name, AtFlags::empty())?;
            }
            fs::unlinkat(&dir, "files", AtFlags::REMOVEDIR)?;
        }
        Err(UploadError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match fs::unlinkat(&dir, "record.next", AtFlags::empty()) {
        Ok(()) | Err(rustix::io::Errno::NOENT) => {}
        Err(error) => return Err(error.into()),
    }
    fs::unlinkat(&dir, "record.json", AtFlags::empty())?;
    fs::unlinkat(root, id, AtFlags::REMOVEDIR)?;
    root.sync_all()?;
    Ok(true)
}
