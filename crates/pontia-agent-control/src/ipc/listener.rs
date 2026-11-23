use crate::AgentControl;
use pontia_core::{Error, Result};
use std::{
    fs::{File, Metadata},
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::watch,
    task::JoinSet,
};

pub struct ControlListener {
    listener: UnixListener,
    path: PathBuf,
    socket: Metadata,
    owner: u32,
    lock: File,
}

// Pi plugins, Claude hosts and ACP relays keep computing this path with their startup
// version across pontiad upgrades. Moving it in 9d550ec6 from $PONTIA_HOME/state/pi/rpc.sock
// left running agents retrying the old path, disconnected and impossible to close from Pontia.
// /tmp fits the ~104–108 byte Unix socket limit even with a long PONTIA_HOME.
// Changing this path requires project owner approval.
pub fn control_socket_path(pontia_home: &Path) -> Result<PathBuf> {
    let home = std::fs::metadata(pontia_home)?;
    Ok(format_socket_path(home.uid(), home.dev(), home.ino()))
}

fn format_socket_path(uid: u32, dev: u64, ino: u64) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/pontia-control-{uid}-{dev:x}-{ino:x}/rpc.sock"
    ))
}

fn same_inode(a: &Metadata, b: &Metadata) -> bool {
    (a.dev(), a.ino()) == (b.dev(), b.ino())
}

impl ControlListener {
    pub async fn bind(pontia_home: &Path) -> Result<Self> {
        std::fs::create_dir_all(pontia_home)?;
        let path = control_socket_path(pontia_home)?;
        Self::bind_path(path, std::fs::metadata(pontia_home)?.uid(), None).await
    }

    async fn bind_path(path: PathBuf, owner: u32, held: Option<&File>) -> Result<Self> {
        let directory = path.parent().expect("socket directory");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        let metadata = std::fs::symlink_metadata(directory)?;
        if !metadata.file_type().is_dir() || metadata.uid() != owner {
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
        let lock = if let Some(held) = held
            && same_inode(&held.metadata()?, &lock.metadata()?)
        {
            held.try_clone()?
        } else {
            lock.try_lock().map_err(|error| {
                Error::StateConflict(format!(
                    "Control listener is already running or cannot be locked: {error}"
                ))
            })?;
            lock
        };
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err(Error::Domain(
                    "Control socket path is occupied by a non-socket file".into(),
                ));
            }
            match tokio::time::timeout(Duration::from_millis(250), UnixStream::connect(&path)).await
            {
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
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
        let socket = std::fs::symlink_metadata(&path)?;
        Ok(Self {
            listener,
            path,
            socket,
            owner,
            lock,
        })
    }

    fn owns_socket(&self) -> bool {
        std::fs::symlink_metadata(&self.path).is_ok_and(|m| same_inode(&self.socket, &m))
    }

    async fn repair(&mut self) -> Result<()> {
        if !self.owns_socket() {
            *self = Self::bind_path(self.path.clone(), self.owner, Some(&self.lock)).await?;
        }
        Ok(())
    }

    pub async fn run(
        mut self,
        state: AgentControl,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<()> {
        let mut maintenance = tokio::time::interval(Duration::from_secs(2));
        let mut connections = JoinSet::new();
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = maintenance.tick() => {
                    if let Err(error) = self.repair().await {
                        tracing::warn!(%error, path = %self.path.display(), "Could not restore control listener");
                    }
                }
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
        if self.owns_socket() {
            let _ = std::fs::remove_file(&self.path);
            let directory = self.path.parent().expect("socket directory");
            let lock = directory.join("rpc.lock");
            if let Ok(held) = self.lock.metadata()
                && std::fs::symlink_metadata(&lock).is_ok_and(|m| same_inode(&held, &m))
            {
                let _ = std::fs::remove_file(lock);
            }
            let _ = std::fs::remove_dir(directory);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pi plugins, Claude hosts and ACP relays keep computing this path with their startup
    // version across pontiad upgrades. Moving it in 9d550ec6 from $PONTIA_HOME/state/pi/rpc.sock
    // left running agents retrying the old path, disconnected and impossible to close from Pontia.
    // /tmp fits the ~104–108 byte Unix socket limit even with a long PONTIA_HOME.
    // Changing this path requires project owner approval.
    #[test]
    fn control_socket_path_is_stable() {
        assert_eq!(
            format_socket_path(1001, 0xabcd, 0x123456789abcdef),
            Path::new("/tmp/pontia-control-1001-abcd-123456789abcdef/rpc.sock")
        );
    }

    #[tokio::test]
    async fn repairs_unlinked_socket_and_directory_without_dropping_connections() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control/rpc.sock");
        let owner = root.path().metadata().unwrap().uid();
        let mut listener = ControlListener::bind_path(path.clone(), owner, None)
            .await
            .unwrap();
        let client = UnixStream::connect(&path).await.unwrap();
        let (server, _) = listener.listener.accept().await.unwrap();
        for remove_directory in [false, true] {
            std::fs::remove_file(&path).unwrap();
            if remove_directory {
                std::fs::remove_file(path.with_file_name("rpc.lock")).unwrap();
                std::fs::remove_dir(path.parent().unwrap()).unwrap();
            }
            listener.repair().await.unwrap();
            assert!(
                ControlListener::bind_path(path.clone(), owner, None)
                    .await
                    .is_err()
            );
            let _new = UnixStream::connect(&path).await.unwrap();
            listener.listener.accept().await.unwrap();
            client.writable().await.unwrap();
            client.try_write(b"x").unwrap();
            server.readable().await.unwrap();
            let mut byte = [0];
            server.try_read(&mut byte).unwrap();
            assert_eq!(byte, *b"x");
        }
    }
}
