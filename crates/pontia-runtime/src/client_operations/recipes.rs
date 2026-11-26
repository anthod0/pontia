use super::{InstallOperation, Phase};
use crate::executable::is_executable;
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{
    fs,
    io::{self, Read},
    os::unix::{fs::symlink, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const SCRIPT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct ScriptSource {
    pub url: &'static str,
    pub allowed_hosts: &'static [&'static str],
}

impl ScriptSource {
    fn permits(&self, url: &Url) -> bool {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| self.allowed_hosts.contains(&host))
    }

    fn fetch(self) -> io::Result<impl Read> {
        let url = Url::parse(self.url).map_err(io::Error::other)?;
        if !self.permits(&url) {
            return Err(io::Error::other("Untrusted install script URL"));
        }
        let client = Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .redirect(Policy::custom(move |attempt| {
                if attempt.previous().len() < 5 && self.permits(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("Untrusted redirect or too many redirects")
                }
            }))
            .build()
            .map_err(io::Error::other)?;
        client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(io::Error::other)
    }
}

#[derive(Clone, Copy)]
pub enum PackageManager {
    Npm,
    Bun,
}

impl InstallOperation {
    fn installing(&self) -> io::Result<()> {
        if self.state.phase != Phase::Installing {
            return Err(io::Error::other(
                "Install recipe requires the installing phase",
            ));
        }
        self.checkpoint()
    }

    pub fn install_script(&self, source: &ScriptSource) -> io::Result<()> {
        self.installing()?;
        self.execute_script(source.fetch()?)
    }

    fn execute_script(&self, reader: impl Read) -> io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.dir)?;
        let size = io::copy(&mut reader.take(SCRIPT_LIMIT as u64 + 1), &mut file)?;
        if size > SCRIPT_LIMIT as u64 {
            return Err(io::Error::other("Install script is too large"));
        }
        let text = fs::read_to_string(file.path())?;
        if text.contains('\0') {
            return Err(io::Error::other("Expected a text install script"));
        }
        let shell = match text.lines().next() {
            Some("#!/bin/sh" | "#!/usr/bin/env sh") => "/bin/sh",
            Some("#!/bin/bash" | "#!/usr/bin/bash" | "#!/usr/bin/env bash") => "/bin/bash",
            _ => return Err(io::Error::other("Expected a shell install script")),
        };
        self.command(Command::new(shell).arg(file.path()).current_dir(&self.home))
    }

    pub fn install_package(
        &self,
        manager: PackageManager,
        client: &str,
        package: &str,
        binary: &str,
        ignore_scripts: bool,
    ) -> io::Result<()> {
        self.installing()?;
        for name in [client, binary] {
            if Path::new(name).file_name().and_then(|v| v.to_str()) != Some(name) {
                return Err(io::Error::other(
                    "Invalid install directory or executable name",
                ));
            }
        }
        let tools = self.home.join("tools");
        let bin = tools.join("bin");
        fs::create_dir_all(&bin)?;
        let destination = tools.join(client);
        let link = bin.join(binary);
        for path in [&destination, &link] {
            match path.symlink_metadata() {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        path.display().to_string(),
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let stage = tempfile::tempdir_in(&tools)?;
        let prefix = stage.path().join("package");
        fs::create_dir(&prefix)?;
        fs::write(prefix.join("package.json"), r#"{"private":true}"#)?;
        let mut command = match manager {
            PackageManager::Npm => {
                let mut cmd = Command::new("npm");
                cmd.args([
                    "install",
                    "--no-global",
                    "--no-audit",
                    "--no-fund",
                    "--prefix",
                ])
                .arg(&prefix);
                cmd
            }
            PackageManager::Bun => {
                let mut cmd = Command::new("bun");
                cmd.arg("add");
                cmd
            }
        };
        command.current_dir(&prefix);
        if ignore_scripts {
            command.arg("--ignore-scripts");
        }
        self.command(command.arg("--").arg(package))?;
        let entry = Path::new("node_modules/.bin").join(binary);
        let installed = prefix.join(&entry);
        if !installed
            .canonicalize()?
            .starts_with(prefix.canonicalize()?)
            || !is_executable(&installed).map_err(io::Error::other)?
        {
            return Err(io::Error::other("Package did not install its executable"));
        }
        self.checkpoint()?;
        fs::rename(&prefix, &destination)?;
        if let Err(error) = symlink(Path::new("..").join(client).join(entry), link) {
            fs::rename(destination, prefix)?;
            return Err(error);
        }
        Ok(())
    }

    fn command(&self, command: &mut Command) -> io::Result<()> {
        self.checkpoint()?;
        command.env_clear();
        for key in [
            "HOME",
            "PATH",
            "USER",
            "LOGNAME",
            "LANG",
            "TMPDIR",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "no_proxy",
            "all_proxy",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "NODE_EXTRA_CA_CERTS",
            "NPM_CONFIG_REGISTRY",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command
            .env("CI", "1")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(self.log.try_clone()?)
            .stderr(self.log.try_clone()?)
            .process_group(0)
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(600);
        let result = loop {
            if let Err(error) = self.checkpoint() {
                break Err(error);
            }
            if Instant::now() >= deadline {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Installer timed out",
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    return Err(io::Error::other(format!("Installer exited with {status}")));
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(error) => break Err(error),
            }
        };
        // Keep the group leader unreaped until escalation so its PID cannot be reused.
        let group = rustix::process::Pid::from_child(&child);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
        thread::sleep(Duration::from_secs(5));
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        child.wait()?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_operations::{Status, cancel, run};

    #[test]
    fn only_valid_shell_text_executes_and_temporary_files_are_removed() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let done = run(home, "client", "key", "script", |op| {
            op.advance(Phase::Installing)?;
            for invalid in [
                &b"#!/bin/sh\n\0"[..],
                b"<html>",
                b"",
                &vec![b'x'; SCRIPT_LIMIT + 1],
            ] {
                assert!(op.execute_script(invalid).is_err());
            }
            for url in ["http://vendor.test/install", "https://other.test/install"] {
                assert!(
                    op.install_script(&ScriptSource {
                        url,
                        allowed_hosts: &["vendor.test"]
                    })
                    .is_err()
                );
            }
            assert!(
                op.execute_script(&b"#!/bin/sh\nprintf '%s' \"$0\" > failed\nexit 1"[..])
                    .is_err()
            );
            op.execute_script(&b"#!/bin/sh\nprintf '%s' \"$0\" > succeeded"[..])?;
            op.advance(Phase::Verifying)
        })
        .unwrap();
        assert_eq!(done.status, Status::Succeeded);
        for marker in ["failed", "succeeded"] {
            let script = fs::read_to_string(home.join(marker)).unwrap();
            assert!(!Path::new(&script).exists());
        }
    }

    #[test]
    fn cancellation_stops_the_install_process_group_and_cleans_up() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        thread::scope(|scope| {
            let (tx, rx) = std::sync::mpsc::channel();
            let handle = scope.spawn(move || run(home, "client", "key", "script", |op| {
                op.advance(Phase::Installing)?;
                tx.send(op.id()).unwrap();
                op.execute_script(&b"#!/bin/sh\ntrap '' TERM\nsleep 60 &\necho $! > child\necho \"$0\" > script\nwait"[..])
            }).unwrap());
            let id = rx.recv().unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !home.join("script").exists() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            let pid = fs::read_to_string(home.join("child"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let child = crate::ProcessFingerprint::capture(pid).unwrap();
            cancel(home, id).unwrap();
            assert_eq!(handle.join().unwrap().status, Status::Cancelled);
            assert_eq!(child.observe(), crate::process::ProcessObservation::Exited);
            assert!(!Path::new(fs::read_to_string(home.join("script")).unwrap().trim()).exists());
        });
    }
}
