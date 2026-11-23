use std::{
    fs::File,
    os::unix::{fs::MetadataExt, net::UnixDatagram},
    path::Path,
};

use pontia_core::{Error, Result};

pub struct LaunchLock(Option<File>);

impl Drop for LaunchLock {
    fn drop(&mut self) {
        // A concurrent fork can briefly retain the open file description until
        // exec closes inherited descriptors. Release at the end of this operation.
        if let Some(file) = &self.0 {
            let _ = file.unlock();
        }
    }
}

// The lock spans spawn and publication of its identity. During shutdown the CLI
// holds the exclusive side until the service manager confirms the service stopped.
pub fn launch(root: &Path) -> Result<LaunchLock> {
    let file = open(root)?;
    file.try_lock_shared().map_err(|error| {
        Error::StateConflict(format!("cannot start an agent during shutdown: {error}"))
    })?;
    Ok(LaunchLock(Some(file)))
}

pub fn shutdown(root: &Path) -> Result<LaunchLock> {
    let file = open(root)?;
    file.try_lock().map_err(|error| {
        Error::StateConflict(format!(
            "agent launch or shutdown in progress, retry shortly: {error}"
        ))
    })?;
    Ok(LaunchLock(Some(file)))
}

fn open(root: &Path) -> Result<File> {
    let state = root.join("state");
    std::fs::create_dir_all(&state)?;
    Ok(File::open(state)?)
}

// The service manager can stop pontiad before the CLI finishes its lifecycle
// operation. Transfer the already-locked open file description across that boundary.
impl LaunchLock {
    pub fn hand_off(mut self, sender: &UnixDatagram) -> std::io::Result<()> {
        use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags, sendmsg};
        use std::{io::IoSlice, mem::MaybeUninit, os::fd::AsFd};

        let fds = [self.0.as_ref().unwrap().as_fd()];
        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut space);
        control.push(SendAncillaryMessage::ScmRights(&fds));
        sendmsg(
            sender,
            &[IoSlice::new(b"L")],
            &mut control,
            SendFlags::DONTWAIT,
        )?;
        self.0.take();
        Ok(())
    }

    pub fn receive(socket: &UnixDatagram) -> std::io::Result<Self> {
        use rustix::net::{RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, recvmsg};
        use std::{
            io::{Error, IoSliceMut},
            mem::MaybeUninit,
        };

        let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let mut byte = [0u8];
        recvmsg(
            socket,
            &mut [IoSliceMut::new(&mut byte)],
            &mut control,
            RecvFlags::DONTWAIT,
        )?;
        for message in control.drain() {
            if let RecvAncillaryMessage::ScmRights(mut fds) = message
                && let Some(fd) = fds.next()
            {
                let file = File::from(fd);
                rustix::io::fcntl_setfd(&file, rustix::io::FdFlags::CLOEXEC)?;
                return Ok(Self(Some(file)));
            }
        }
        Err(Error::other("missing shutdown lock"))
    }
}

pub fn receiver_directory(id: uuid::Uuid) -> std::path::PathBuf {
    Path::new("/tmp").join(format!("pontia-shutdown-{id}"))
}

pub fn connect_receiver(receiver: uuid::Uuid) -> std::io::Result<UnixDatagram> {
    let sender = UnixDatagram::unbound()?;
    let directory = receiver_directory(receiver);
    let meta = directory.symlink_metadata()?;
    if !meta.is_dir()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o077 != 0
    {
        return Err(std::io::Error::other(
            "shutdown lock receiver is not private",
        ));
    }
    sender.connect(directory.join("lock"))?;
    Ok(sender)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_excludes_launches_until_the_service_manager_releases_the_lock() {
        let root = tempfile::tempdir().unwrap();
        let (sender, receiver) = UnixDatagram::pair().unwrap();
        shutdown(root.path()).unwrap().hand_off(&sender).unwrap();
        assert!(launch(root.path()).is_err());
        let guard = LaunchLock::receive(&receiver).unwrap();
        assert!(launch(root.path()).is_err());
        drop(guard);
        assert!(launch(root.path()).is_ok());
    }
}
