use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
};

#[cfg(target_os = "macos")]
mod macos;

use pontia_core::{
    Error, Result,
    client_capabilities::AgentClientCapabilities,
    ids::{new_event_id, new_runtime_id},
};
use serde_json::json;

use crate::{
    GenericRuntimeManager, ProcessFingerprint, ProcessObservation, RuntimeStartRequest,
    RuntimeStartResult, paths,
};

fn stdin_fifo(directory: &Path) -> Result<File> {
    let path = directory.join(format!("stdin-{}", new_event_id()));
    let name = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    // The child holds both ends. Daemon shutdown cannot deliver EOF.
    let file = OpenOptions::new().read(true).write(true).open(&path);
    std::fs::remove_file(path)?;
    Ok(file?)
}

pub(crate) fn independent_command(cli: &Path, description: &str) -> std::io::Result<Command> {
    #[cfg(target_os = "linux")]
    if Path::new("/run/systemd/system").is_dir() {
        let mut command = Command::new("systemd-run");
        let help = Command::new("systemd-run").arg("--help").output()?;
        if !help.status.success() {
            return Err(std::io::Error::other(
                String::from_utf8_lossy(&help.stderr).into_owned(),
            ));
        }
        if String::from_utf8_lossy(&help.stdout).contains("--expand-environment=") {
            command.arg("--expand-environment=no");
        }
        command.args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--no-ask-password",
            &format!("--description={description}"),
            "--",
        ]);
        command.arg(cli);
        return Ok(command);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = description;
    Ok(Command::new(cli))
}

/// Build a command that can outlive the service manager's cleanup of pontiad.
pub fn detached_command(program: &Path, description: &str) -> io::Result<Command> {
    let mut command = independent_command(program, description)?;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(command)
}

/// Release the caller's child handle while retaining the OS ownership needed to reap it.
pub fn reap_detached_child(child: std::process::Child) {
    #[cfg(target_os = "macos")]
    macos::reap(child);
    #[cfg(not(target_os = "macos"))]
    std::thread::spawn(move || {
        let mut child = child;
        child.wait()
    });
}

impl GenericRuntimeManager {
    pub fn start_detached_process(
        &self,
        root: &Path,
        request: RuntimeStartRequest,
        capabilities: AgentClientCapabilities,
    ) -> Result<RuntimeStartResult> {
        let workspace = paths::workspace_path(root, &request)?;
        let logs = paths::log_paths(root);
        std::fs::create_dir_all(&logs.log_dir)?;
        let stderr = pontia_core::private_fs::append_log(&logs.runtime_log)?;
        let command = request
            .start_command
            .as_deref()
            .ok_or_else(|| Error::Domain("detached launch requires a command".into()))?;
        let mut child = detached_command(Path::new("/bin/bash"), "Pontia Session")?;
        child
            .arg("-c")
            .arg(command)
            .current_dir(&workspace)
            .stdin(stdin_fifo(&logs.log_dir)?)
            .stdout(Stdio::null())
            .stderr(stderr);
        for name in crate::script::SESSION_ENVIRONMENT.split_ascii_whitespace() {
            child.env_remove(name);
        }
        crate::script::validate_runtime_environment(&request)?;
        child.env("PONTIA_HOME", root).envs(&request.environment);
        let mut child = child.spawn()?;
        let Some(fingerprint) = ProcessFingerprint::capture(child.id()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::CapabilityUnavailable(
                "cannot identify detached process".into(),
            ));
        };
        reap_detached_child(child);
        let runtime_id = request
            .runtime_id
            .unwrap_or_else(|| new_runtime_id().to_string());
        Ok(RuntimeStartResult {
            runtime_kind: "process".into(),
            runtime_handle: runtime_id.clone(),
            capabilities,
            metadata: json!({"runtime_id":runtime_id, "launch_cwd":workspace, "process_fingerprint":fingerprint,
                "runtime_log":logs.runtime_log, "start_command":command}),
        })
    }

    pub fn terminate_process(&self, fingerprint: &ProcessFingerprint) -> Result<()> {
        self.signal(fingerprint, libc::SIGTERM)
    }

    pub fn kill_process(&self, fingerprint: &ProcessFingerprint) -> Result<()> {
        self.signal(fingerprint, libc::SIGKILL)
    }

    fn signal(&self, fingerprint: &ProcessFingerprint, signal: i32) -> Result<()> {
        let pid = i32::try_from(fingerprint.pid)
            .ok()
            .filter(|pid| *pid > 0)
            .ok_or_else(|| Error::Domain("invalid process PID".into()))?;
        #[cfg(target_os = "macos")]
        let target = match macos::SignalTarget::open(pid) {
            Ok(target) => target,
            Err(_) if fingerprint.observe() == ProcessObservation::Exited => return Ok(()),
            Err(error) => return Err(error),
        };
        // Pin the kernel process object before checking the persisted identity.
        #[cfg(target_os = "linux")]
        let pidfd = {
            use std::os::fd::{FromRawFd, OwnedFd};
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
            if fd < 0 {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(libc::ESRCH) {
                    Ok(())
                } else {
                    Err(error.into())
                };
            }
            unsafe { OwnedFd::from_raw_fd(fd as i32) }
        };
        match fingerprint.observe() {
            ProcessObservation::Exited => return Ok(()),
            ProcessObservation::Unknown => {
                return Err(Error::ControlUnknown(
                    "cannot verify process identity".into(),
                ));
            }
            ProcessObservation::Alive => {}
        }
        #[cfg(target_os = "linux")]
        let status = unsafe {
            use std::os::fd::AsRawFd;
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        #[cfg(target_os = "macos")]
        target.signal(signal)?;
        #[cfg(target_os = "linux")]
        if status != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_process_owns_its_session_and_fifo_until_verified_termination() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("check.py"),
            r#"
import os, select, stat, sys, time
assert os.environ["PONTIA_TEST_CHILD"] == "1"
assert os.getsid(0) == os.getpid() == os.getpgrp()
assert stat.S_ISFIFO(os.fstat(0).st_mode) and os.fstat(0).st_nlink == 0
assert not select.select([0], [], [], 0.05)[0]
assert os.fstat(1).st_rdev == os.stat(os.devnull).st_rdev
print('ready', file=sys.stderr, flush=True)
time.sleep(10)
"#,
        )
        .unwrap();
        let request = serde_json::from_value(json!({"client_type":"test","session_id":"session","workspace":root.path(),"start_command":"cd . && PONTIA_TEST_CHILD=1 python3 check.py"})).unwrap();
        let manager = GenericRuntimeManager;
        let result = manager
            .start_detached_process(root.path(), request, Default::default())
            .unwrap();
        let process: ProcessFingerprint =
            serde_json::from_value(result.metadata["process_fingerprint"].clone()).unwrap();
        let log = root.path().join("state/runtime.log");
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &std::fs::metadata(&log).unwrap().permissions()
            ) & 0o777,
            0o600
        );
        for _ in 0..200 {
            if std::fs::read_to_string(&log).unwrap() == "ready\n" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(log).unwrap(), "ready\n");
        let mut wrong = process.clone();
        wrong.start_time_seconds += 1;
        manager.terminate_process(&wrong).unwrap();
        assert_eq!(process.observe(), ProcessObservation::Alive);
        manager.terminate_process(&process).unwrap();
        for _ in 0..100 {
            if process.observe() == ProcessObservation::Exited {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("detached process survived termination");
    }
}
