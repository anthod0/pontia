use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::lifecycle::{DefinitionStore, HealthProbe};

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Default)]
pub struct FileDefinitionStore;

impl DefinitionStore for FileDefinitionStore {
    fn read(&self, path: &Path) -> Result<Option<String>, String> {
        match fs::read_to_string(path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("failed to read {}: {error}", path.display())),
        }
    }

    fn install(&self, path: &Path, contents: &str) -> Result<bool, String> {
        if self.read(path)?.as_deref() == Some(contents) {
            return Ok(false);
        }
        let parent = path
            .parent()
            .ok_or_else(|| format!("service definition path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

        let mut last_collision = None;
        for _ in 0..32 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temp_path = parent.join(format!(
                ".pontia-definition-{}-{sequence}.tmp",
                std::process::id()
            ));
            match write_and_replace(&temp_path, path, contents) {
                Ok(()) => return Ok(true),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    last_collision = Some(error);
                }
                Err(error) => {
                    let _ = fs::remove_file(&temp_path);
                    return Err(format!(
                        "failed to atomically install {}: {error}",
                        path.display()
                    ));
                }
            }
        }
        Err(format!(
            "failed to create a staging file for {}: {}",
            path.display(),
            last_collision
                .map(|error| error.to_string())
                .unwrap_or_else(|| "too many name collisions".to_string())
        ))
    }
}

fn write_and_replace(temp_path: &Path, target: &Path, contents: &str) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }
    let mut file = options.open(temp_path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp_path, target)?;
    if let Some(parent) = target.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct HttpHealthProbe;

impl HealthProbe for HttpHealthProbe {
    fn wait_until_healthy(
        &self,
        addr: SocketAddr,
        timeout: Duration,
        keep_waiting: &mut dyn FnMut() -> Result<bool, String>,
    ) -> Result<bool, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.is_healthy(addr)? {
                return Ok(true);
            }
            if !keep_waiting()? {
                return Ok(false);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(200).min(deadline - now));
        }
    }

    fn is_healthy(&self, addr: SocketAddr) -> Result<bool, String> {
        let timeout = Duration::from_secs(1);
        let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => stream,
            Err(_) => return Ok(false),
        };
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| format!("failed to configure health connection: {error}"))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|error| format!("failed to configure health connection: {error}"))?;
        if write!(
            stream,
            "GET /healthz HTTP/1.0\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
        )
        .is_err()
        {
            return Ok(false);
        }
        let mut response = Vec::new();
        if stream.read_to_end(&mut response).is_err() {
            return Ok(false);
        }
        let response = match String::from_utf8(response) {
            Ok(response) => response,
            Err(_) => return Ok(false),
        };
        let Some((head, body)) = response.split_once("\r\n\r\n") else {
            return Ok(false);
        };
        let successful = head.lines().next().is_some_and(|line| {
            line.starts_with("HTTP/1.0 200 ") || line.starts_with("HTTP/1.1 200 ")
        });
        Ok(successful && body.trim() == r#"{"status":"ok"}"#)
    }
}
