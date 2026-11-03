use pontia_application::uploads::{MAX_CHUNK_SIZE, MAX_UPLOAD_SIZE, UploadError, UploadService};
use sha2::{Digest, Sha256};
use std::{fs, sync::Arc};
const ID: &str = "12345678-1234-4234-8234-123456789abc";
fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn restart_resumes_only_confirmed_bytes_and_completion_is_stable() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    let (created, fresh) = service
        .create(ID, "record.json".into(), 6, checksum(b"abcdef"))
        .unwrap();
    assert!(fresh);
    assert_eq!(service.append(ID, 0, b"abc").unwrap().offset, 3);
    let file = root
        .path()
        .join("state/uploads")
        .join(ID)
        .join("files/record.json");
    fs::write(&file, b"abcunconfirmed bytes").unwrap();
    let restarted = UploadService::new(root.path().into());
    let (recovered, fresh) = restarted
        .create(
            &ID.to_uppercase(),
            "record.json".into(),
            6,
            checksum(b"abcdef"),
        )
        .unwrap();
    assert!(!fresh);
    assert_eq!(recovered.offset, 3);
    assert_eq!(recovered.created_at, created.created_at);
    assert!(matches!(
        restarted.append(ID, 0, b"abc"),
        Err(UploadError::OffsetConflict)
    ));
    assert!(matches!(
        restarted.complete(ID),
        Err(UploadError::Incomplete)
    ));
    restarted.append(ID, 3, b"def").unwrap();
    let complete = restarted.complete(ID).unwrap();
    assert!(complete.completed);
    assert_eq!(
        fs::read(complete.path.as_ref().unwrap()).unwrap(),
        b"abcdef"
    );
    assert_eq!(restarted.complete(ID).unwrap().path, complete.path);
    assert_eq!(restarted.get(ID).unwrap().path, complete.path);
    assert!(matches!(
        restarted.append(ID, 6, b"x"),
        Err(UploadError::Completed)
    ));
}

#[test]
fn empty_files_complete_and_bad_checksums_never_expose_a_path() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    service
        .create(ID, "empty".into(), 0, checksum(b""))
        .unwrap();
    assert!(service.complete(ID).unwrap().completed);
    let other = "22345678-1234-4234-8234-123456789abc";
    service
        .create(other, "wrong".into(), 1, checksum(b"a"))
        .unwrap();
    service.append(other, 0, b"b").unwrap();
    assert!(matches!(
        service.complete(other),
        Err(UploadError::ChecksumMismatch)
    ));
    assert_eq!(service.get(other).unwrap().path, None);
}

#[test]
fn rejects_invalid_names_limits_and_conflicting_metadata() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    for name in ["", ".", "..", "a/b", "a\\b", "a\0b"] {
        assert!(matches!(
            service.create(ID, name.into(), 0, checksum(b"")),
            Err(UploadError::Invalid)
        ));
    }
    assert!(matches!(
        service.create(ID, "x".into(), MAX_UPLOAD_SIZE + 1, checksum(b"")),
        Err(UploadError::TooLarge)
    ));
    service.create(ID, "x".into(), 1, checksum(b"a")).unwrap();
    assert!(matches!(
        service.create(ID, "y".into(), 1, checksum(b"a")),
        Err(UploadError::IdempotencyConflict)
    ));
    assert!(matches!(
        service.append(ID, 0, &vec![0; MAX_CHUNK_SIZE + 1]),
        Err(UploadError::TooLarge)
    ));
    assert!(matches!(
        service.append(ID, 0, b"ab"),
        Err(UploadError::Invalid)
    ));
    assert_eq!(service.get(ID).unwrap().offset, 0);
}

#[test]
fn concurrent_requests_cannot_both_append_from_the_same_offset() {
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    service.create(ID, "x".into(), 2, checksum(b"ab")).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let service = service.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                service.append(ID, 0, b"a")
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(service.get(ID).unwrap().offset, 1);
}

#[test]
fn symlinks_cannot_redirect_file_or_record_writes() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let service = UploadService::new(root.path().into());
    service.create(ID, "x".into(), 1, checksum(b"a")).unwrap();
    let dir = root.path().join("state/uploads").join(ID);
    let outside = root.path().join("outside");
    fs::write(&outside, b"preserve").unwrap();
    fs::remove_file(dir.join("files/x")).unwrap();
    symlink(&outside, dir.join("files/x")).unwrap();
    assert!(matches!(
        service.append(ID, 0, b"a"),
        Err(UploadError::Io(_))
    ));
    assert_eq!(service.get(ID).unwrap().offset, 0);
    fs::remove_file(dir.join("files/x")).unwrap();
    fs::write(dir.join("files/x"), b"").unwrap();
    symlink(&outside, dir.join("record.next")).unwrap();
    assert!(matches!(
        service.append(ID, 0, b"a"),
        Err(UploadError::Io(_))
    ));
    assert_eq!(service.get(ID).unwrap().offset, 0);
    assert_eq!(fs::read(outside).unwrap(), b"preserve");
}
