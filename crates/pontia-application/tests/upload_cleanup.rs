use std::{fs, path::Path};

use pontia_application::uploads::{Upload, UploadError, UploadService};
use pontia_config::UploadsConfig;
use rustix::fs::{FlockOperation, flock};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

const FIRST: &str = "12345678-1234-4234-8234-123456789abc";
const SECOND: &str = "22345678-1234-4234-8234-123456789abc";
const CREATED: &str = "2026-01-01T00:00:00Z";

fn now(days: i64) -> OffsetDateTime {
    OffsetDateTime::parse(CREATED, &Rfc3339).unwrap() + Duration::days(days)
}

fn upload(root: &Path, id: &str, complete: bool) -> Upload {
    let service = UploadService::new(root.into());
    service
        .create(
            id,
            "file.txt".into(),
            3,
            format!("{:x}", Sha256::digest(b"abc")),
        )
        .unwrap();
    service.append(id, 0, b"abc").unwrap();
    let mut upload = if complete {
        service.complete(id)
    } else {
        service.get(id)
    }
    .unwrap();
    upload.created_at = CREATED.into();
    fs::write(
        root.join("state/uploads").join(id).join("record.json"),
        serde_json::to_vec(&upload).unwrap(),
    )
    .unwrap();
    upload
}

#[test]
fn cleanup_uses_separate_retention_periods_from_creation_time() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    upload(root.path(), FIRST, false);
    let completed = upload(root.path(), SECOND, true);
    let config = UploadsConfig::default();

    assert_eq!(
        service
            .cleanup(&config, now(7) - Duration::seconds(1))
            .unwrap(),
        0
    );
    assert_eq!(service.cleanup(&config, now(7)).unwrap(), 1);
    assert!(matches!(service.get(FIRST), Err(UploadError::NotFound)));
    assert!(!root.path().join("state/uploads").join(FIRST).exists());
    assert_eq!(fs::read(completed.path.as_ref().unwrap()).unwrap(), b"abc");
    assert_eq!(
        service
            .cleanup(&config, now(30) - Duration::seconds(1))
            .unwrap(),
        0
    );
    assert_eq!(service.cleanup(&config, now(30)).unwrap(), 1);
    assert!(matches!(service.get(SECOND), Err(UploadError::NotFound)));
    assert!(!Path::new(completed.path.as_ref().unwrap()).exists());
    assert_eq!(service.cleanup(&config, now(31)).unwrap(), 0);
}

#[test]
fn configured_retention_changes_when_files_are_removed() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    upload(root.path(), FIRST, false);
    upload(root.path(), SECOND, true);
    let config = UploadsConfig {
        incomplete_retention_days: 2.try_into().unwrap(),
        completed_retention_days: 3.try_into().unwrap(),
        ..UploadsConfig::default()
    };
    assert_eq!(service.cleanup(&config, now(2)).unwrap(), 1);
    assert!(matches!(service.get(FIRST), Err(UploadError::NotFound)));
    assert!(service.get(SECOND).unwrap().completed);
    assert_eq!(service.cleanup(&config, now(3)).unwrap(), 1);
    assert!(matches!(service.get(SECOND), Err(UploadError::NotFound)));
}

#[test]
fn cleanup_skips_locked_uploads_and_retries_after_the_writer_finishes() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    upload(root.path(), FIRST, false);
    upload(root.path(), SECOND, true);
    let lock = fs::File::open(root.path().join("state/uploads").join(FIRST)).unwrap();
    flock(&lock, FlockOperation::LockExclusive).unwrap();
    assert_eq!(
        service.cleanup(&UploadsConfig::default(), now(31)).unwrap(),
        1
    );
    assert!(
        root.path()
            .join("state/uploads")
            .join(FIRST)
            .join("files/file.txt")
            .exists()
    );
    drop(lock);
    assert_eq!(
        service.cleanup(&UploadsConfig::default(), now(31)).unwrap(),
        1
    );
    assert!(matches!(service.get(FIRST), Err(UploadError::NotFound)));
}

#[test]
fn cleanup_does_not_follow_upload_directory_or_file_symlinks() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("pontia");
    fs::create_dir(&home).unwrap();
    let service = UploadService::new(home.clone());
    upload(&home, FIRST, true);
    let uploads = home.join("state/uploads");
    let outside = root.path().join("outside");
    fs::rename(uploads.join(FIRST), &outside).unwrap();
    symlink(&outside, uploads.join(FIRST)).unwrap();
    assert_eq!(
        service.cleanup(&UploadsConfig::default(), now(31)).unwrap(),
        0
    );
    assert_eq!(fs::read(outside.join("files/file.txt")).unwrap(), b"abc");

    upload(&home, SECOND, true);
    let file = uploads.join(SECOND).join("files/file.txt");
    fs::remove_file(&file).unwrap();
    symlink(outside.join("files/file.txt"), &file).unwrap();
    assert_eq!(
        service.cleanup(&UploadsConfig::default(), now(31)).unwrap(),
        1
    );
    assert!(!uploads.join(SECOND).exists());
    assert_eq!(fs::read(outside.join("files/file.txt")).unwrap(), b"abc");
}

#[test]
fn cleanup_preserves_invalid_records_and_continues_with_other_uploads() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    let mut invalid = upload(root.path(), FIRST, false);
    invalid.created_at = "invalid".into();
    let path = root
        .path()
        .join("state/uploads")
        .join(FIRST)
        .join("record.json");
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    upload(root.path(), SECOND, true);
    assert_eq!(
        service.cleanup(&UploadsConfig::default(), now(31)).unwrap(),
        1
    );
    assert_eq!(service.get(FIRST).unwrap().offset, 3);
    assert!(matches!(service.get(SECOND), Err(UploadError::NotFound)));
}
