//! Process groups for short-lived interactive client login commands.
//!
//! Login commands stay within pontiad's lifetime: they are not detached from the
//! service manager, and pontiad terminates the whole group on cancellation.

use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{ProcessFingerprint, ProcessObservation};

/// Build a command whose child leads a new process group, with stdin closed.
pub fn group_command(program: &Path) -> Command {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(program);
    command.process_group(0).stdin(Stdio::null());
    command
}

/// A process group led by a child started from [`group_command`].
#[derive(Debug, Clone)]
pub struct ProcessGroup {
    leader: ProcessFingerprint,
}

impl ProcessGroup {
    /// Capture the leader's identity. Returns `None` when the leader already exited.
    pub fn adopt(pid: u32) -> Option<Self> {
        ProcessFingerprint::capture(pid).map(|leader| Self { leader })
    }

    pub fn pgid(&self) -> u32 {
        self.leader.pid
    }

    /// Send `SIGTERM` to the group, then `SIGKILL` if members remain after `grace`.
    ///
    /// The caller must keep reaping the leader concurrently; an unreaped zombie
    /// leader keeps the group observable until the grace period ends.
    pub fn terminate(&self, grace: Duration) {
        // Once the leader is gone its PID may lead an unrelated group.
        if self.leader.observe() == ProcessObservation::Exited {
            return;
        }
        if !self.signal(libc::SIGTERM) {
            return;
        }
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if !self.has_members() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        self.signal(libc::SIGKILL);
    }

    pub fn has_members(&self) -> bool {
        self.signal(0)
    }

    fn signal(&self, signal: libc::c_int) -> bool {
        let Ok(pgid) = libc::pid_t::try_from(self.leader.pid) else {
            return false;
        };
        unsafe { libc::killpg(pgid, signal) == 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn terminate_stops_the_leader_and_its_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("grandchild");
        let mut command = group_command(Path::new("/bin/sh"));
        command.args([
            "-c",
            &format!("sleep 60 & echo $! > {}; wait", pid_file.display()),
        ]);
        let mut child = command.spawn().unwrap();
        assert!(wait_until(
            || std::fs::read_to_string(&pid_file).is_ok_and(|pid| !pid.trim().is_empty())
        ));
        let grandchild: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let grandchild = ProcessFingerprint::capture(grandchild).unwrap();
        let group = ProcessGroup::adopt(child.id()).unwrap();
        assert_eq!(group.pgid(), child.id());
        let reaper = std::thread::spawn(move || child.wait().unwrap());
        group.terminate(Duration::from_secs(2));
        assert!(!reaper.join().unwrap().success());
        assert!(wait_until(
            || grandchild.observe() == ProcessObservation::Exited
        ));
        assert!(!group.has_members());
    }

    #[test]
    fn terminate_escalates_when_the_group_ignores_sigterm() {
        let mut command = group_command(Path::new("/bin/sh"));
        command.args(["-c", "trap '' TERM; while :; do sleep 0.05; done"]);
        let mut child = command.spawn().unwrap();
        let group = ProcessGroup::adopt(child.id()).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let reaper = std::thread::spawn(move || child.wait().unwrap());
        let started = Instant::now();
        group.terminate(Duration::from_millis(200));
        let status = reaper.join().unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn terminating_an_exited_group_is_a_no_op() {
        let mut child = group_command(Path::new("/bin/true")).spawn().unwrap();
        let group = ProcessGroup::adopt(child.id());
        child.wait().unwrap();
        if let Some(group) = group {
            group.terminate(Duration::from_millis(100));
            assert!(!group.has_members());
        }
    }
}
