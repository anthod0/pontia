use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("private file path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".pontia-private-{}-{sequence}.tmp",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp_path) {
            Ok(mut file) => {
                let result = (|| -> std::io::Result<()> {
                    file.write_all(contents)?;
                    file.sync_all()?;
                    drop(file);
                    fs::rename(&temp_path, path)?;
                    File::open(parent)?.sync_all()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = fs::remove_file(&temp_path);
                    return Err(format!(
                        "failed to atomically write {}: {error}",
                        path.display()
                    ));
                }
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create a staging file for {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Err(format!(
        "failed to create a staging file for {}: too many name collisions",
        path.display()
    ))
}
