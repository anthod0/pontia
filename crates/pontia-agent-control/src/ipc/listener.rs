use crate::AgentControl;
use pontia_core::{Error, Result};
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::watch,
    task::JoinSet,
};

pub struct ControlListener {
    listener: UnixListener,
    path: PathBuf,
    inode: u64,
    owner: u32,
    _lock: std::fs::File,
}

impl ControlListener {
    pub async fn bind(pontia_home: &Path) -> Result<Self> {
        let directory = pontia_home.join("state/pi");
        std::fs::create_dir_all(&directory)?;
        if !std::fs::symlink_metadata(&directory)?.file_type().is_dir() {
            return Err(Error::Domain(
                "Control socket directory must be a real directory".into(),
            ));
        }
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(directory.join("rpc.lock"))?;
        lock.try_lock().map_err(|error| {
            Error::StateConflict(format!(
                "Control listener is already running or cannot be locked: {error}"
            ))
        })?;
        let path = directory.join("rpc.sock");
        if path.as_os_str().as_encoded_bytes().len() > 103 {
            return Err(Error::Domain(
                "Pontia home is too long for the Control Unix socket (maximum 103 bytes)".into(),
            ));
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err(Error::Domain(
                    "Control socket path is occupied by a non-socket file".into(),
                ));
            }
            match UnixStream::connect(&path).await {
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&path)?
                }
                _ => {
                    return Err(Error::StateConflict(
                        "Control socket is already in use".into(),
                    ));
                }
            }
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let metadata = std::fs::metadata(&path)?;
        Ok(Self {
            listener,
            path,
            inode: metadata.ino(),
            owner: metadata.uid(),
            _lock: lock,
        })
    }

    pub async fn run(self, state: AgentControl, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut connections = JoinSet::new();
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted?;
                    if stream.peer_cred()?.uid() != self.owner { continue; }
                    let state = state.clone();
                    connections.spawn(async move { super::serve_connection(state, stream).await; });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        Ok(())
    }
}

impl Drop for ControlListener {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.ino() == self.inode && metadata.file_type().is_socket())
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
