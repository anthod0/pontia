use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Component, Path},
};

use pontia_core::{Error, Result};

pub struct LaunchFile {
    pub path: &'static str,
    pub contents: &'static str,
}

impl LaunchFile {
    pub fn install(&self, directory: &Path) -> Result<()> {
        let relative = Path::new(self.path);
        if relative.file_name().is_none()
            || !relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(Error::Domain(format!(
                "Invalid launch file path: {}",
                self.path
            )));
        }
        let root = directory.canonicalize()?;
        let mut parent = root.clone();
        for component in relative.parent().unwrap().components() {
            parent.push(component.as_os_str());
            match fs::create_dir(&parent) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            if !parent.canonicalize()?.starts_with(&root) {
                return Err(Error::Domain(format!(
                    "Launch file outside the launch directory: {}",
                    self.path
                )));
            }
        }
        let target = root.join(relative);
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(self.contents.as_bytes())?;
        file.persist(target).map_err(|e| e.error)?;
        Ok(())
    }
}
