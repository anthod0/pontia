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

pub fn control_socket_path(pontia_home: &Path) -> Result<PathBuf> {
    let home = std::fs::metadata(pontia_home)?;
    Ok(PathBuf::from(format!(
        "/tmp/pontia-control-{}-{:x}-{:x}",
        home.uid(),
        home.dev(),
        home.ino()
    ))
    .join("rpc.sock"))
}

impl ControlListener {
    pub async fn bind(pontia_home: &Path) -> Result<Self> {
        let path = control_socket_path(pontia_home)?;
        let directory = path.parent().expect("socket directory");
        std::fs::create_dir_all(directory)?;
        let metadata = std::fs::symlink_metadata(directory)?;
        if !metadata.file_type().is_dir() || metadata.uid() != std::fs::metadata(pontia_home)?.uid()
        {
            return Err(Error::Domain(
                "Control socket directory must be a real directory".into(),
            ));
        }
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
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
            let directory = self.path.parent().expect("socket directory");
            let _ = std::fs::remove_file(directory.join("rpc.lock"));
            let _ = std::fs::remove_dir(directory);
        }
    }
}
