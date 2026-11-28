use std::{io, io::Write, path::Path, process::Command};

pub fn write_script(path: &Path, script: &str) -> io::Result<()> {
    let mut source = tempfile::NamedTempFile::new()?;
    writeln!(source, "#!/bin/sh\n{script}")?;
    // Only the installer opens the executable for writing. Concurrent forks
    // of the test process can inherit a writer for the source, never the executable.
    let status = Command::new("install")
        .args(["-m", "700"])
        .arg(source.path())
        .arg(path)
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!("install test script: {status}")));
    }
    Ok(())
}
