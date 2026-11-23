use std::{fs::File, path::Path};

use pontia_core::{Error, Result};

pub struct LaunchLock(File);

impl Drop for LaunchLock {
    fn drop(&mut self) {
        // A concurrent fork can briefly retain the open file description until
        // exec closes inherited descriptors. Release at the end of this operation.
        let _ = self.0.unlock();
    }
}

// The lock spans spawn and publication of its identity. During shutdown the CLI
// holds the exclusive side until the service manager confirms the service stopped.
pub fn launch(root: &Path) -> Result<LaunchLock> {
    let file = open(root)?;
    file.try_lock_shared().map_err(|error| {
        Error::StateConflict(format!("cannot start an agent during shutdown: {error}"))
    })?;
    Ok(LaunchLock(file))
}

pub fn shutdown(root: &Path) -> Result<LaunchLock> {
    let file = open(root)?;
    file.try_lock().map_err(|error| {
        Error::StateConflict(format!(
            "agent launch or shutdown in progress, retry shortly: {error}"
        ))
    })?;
    Ok(LaunchLock(file))
}

fn open(root: &Path) -> Result<File> {
    let state = root.join("state");
    std::fs::create_dir_all(&state)?;
    Ok(File::open(state)?)
}
