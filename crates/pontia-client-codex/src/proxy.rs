use std::{
    io,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
};

use pontia_core::Result;
use tokio::{
    net::{UnixListener, UnixStream},
    sync::watch,
    task::JoinSet,
};

pub(crate) fn socket_path(home: &Path) -> PathBuf {
    home.join("codex-proxy.sock")
}

pub(crate) struct Proxy {
    listener: UnixListener,
    path: PathBuf,
}

impl Proxy {
    pub(crate) async fn bind(home: &Path) -> Result<Self> {
        let path = socket_path(home);
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Codex proxy path is not a socket",
                )
                .into());
            }
            match UnixStream::connect(&path).await {
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&path)?;
                }
                Err(error) => return Err(error.into()),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "Codex proxy is already listening",
                    )
                    .into());
                }
            }
        }
        let listener = UnixListener::bind(&path)?;
        let proxy = Self { listener, path };
        std::fs::set_permissions(&proxy.path, std::fs::Permissions::from_mode(0o600))?;
        Ok(proxy)
    }

    pub(crate) async fn run(
        self,
        mut shutdown: watch::Receiver<bool>,
        upstream: impl Fn() -> Result<PathBuf>,
    ) -> Result<()> {
        let mut connections = JoinSet::new();
        while !*shutdown.borrow() {
            tokio::select! {
                _ = shutdown.changed() => break,
                result = connections.join_next(), if !connections.is_empty() => {
                    match result.expect("nonempty connection set") {
                        Ok(Ok(())) => {},
                        Ok(Err(error)) => tracing::debug!(%error, "Codex proxy connection closed"),
                        Err(error) => tracing::warn!(%error, "Codex proxy connection task failed"),
                    }
                }
                accepted = self.listener.accept() => {
                    let (downstream, _) = accepted?;
                    let endpoint = upstream();
                    connections.spawn(async move {
                        relay(downstream, &endpoint?).await
                    });
                }
            }
        }
        connections.shutdown().await;
        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "Could not remove Codex proxy socket");
        }
    }
}

async fn relay(mut downstream: UnixStream, upstream: &Path) -> Result<()> {
    let mut upstream = UnixStream::connect(upstream).await?;
    let (mut downstream_read, mut downstream_write) = downstream.split();
    let (mut upstream_read, mut upstream_write) = upstream.split();
    // EOF in either direction ends this transport only, without interpreting execution state.
    tokio::select! {
        result = tokio::io::copy(&mut downstream_read, &mut upstream_write) => { result?; }
        result = tokio::io::copy(&mut upstream_read, &mut downstream_write) => { result?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
