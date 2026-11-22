use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use sysinfo::{ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time_seconds: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pub start_time_seconds: u64,
    pub status: ProcessStatus,
}

struct ProcessTable {
    processes: HashMap<u32, ProcessInfo>,
}

impl ProcessTable {
    pub(crate) fn refresh_all() -> Self {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        let processes: HashMap<u32, ProcessInfo> = system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let pid = pid.as_u32();
                (
                    pid,
                    ProcessInfo {
                        start_time_seconds: process.start_time(),
                        status: process.status(),
                    },
                )
            })
            .collect();
        Self { processes }
    }

    pub(crate) fn get(&self, pid: u32) -> Option<&ProcessInfo> {
        self.processes.get(&pid)
    }

    pub(crate) fn observe(&self, identity: ProcessIdentity) -> ProcessObservation {
        let Some(process) = self.get(identity.pid) else {
            let Ok(pid) = libc::pid_t::try_from(identity.pid) else {
                return ProcessObservation::Unknown;
            };
            // A snapshot can omit live processes on macOS. Probe only the target PID.
            return if pid > 0
                && unsafe { libc::kill(pid, 0) } == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                ProcessObservation::Exited
            } else {
                ProcessObservation::Unknown
            };
        };
        if process.start_time_seconds == 0 || matches!(process.status, ProcessStatus::Unknown(_)) {
            return ProcessObservation::Unknown;
        }
        if process.start_time_seconds != identity.start_time_seconds
            || matches!(process.status, ProcessStatus::Zombie | ProcessStatus::Dead)
        {
            ProcessObservation::Exited
        } else {
            ProcessObservation::Alive
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessObservation {
    Alive,
    Exited,
    Unknown,
}

pub fn process_identity(pid: u32) -> Option<ProcessIdentity> {
    let table = ProcessTable::refresh_all();
    let process = table.get(pid)?;
    let identity = ProcessIdentity {
        pid,
        start_time_seconds: process.start_time_seconds,
    };
    (table.observe(identity) == ProcessObservation::Alive).then_some(identity)
}

pub fn system_boot_time_seconds() -> Option<u64> {
    let boot_time = System::boot_time();
    (boot_time != 0).then_some(boot_time)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessFingerprint {
    pub boot_time_seconds: u64,
    pub pid: u32,
    pub start_time_seconds: u64,
}

impl ProcessFingerprint {
    pub fn capture(pid: u32) -> Option<Self> {
        let boot_time_seconds = system_boot_time_seconds()?;
        let identity = process_identity(pid)?;
        Some(Self {
            boot_time_seconds,
            pid,
            start_time_seconds: identity.start_time_seconds,
        })
    }

    pub fn observe(&self) -> ProcessObservation {
        let Some(boot_time) = system_boot_time_seconds() else {
            return ProcessObservation::Unknown;
        };
        if boot_time != self.boot_time_seconds {
            return ProcessObservation::Exited;
        }
        ProcessTable::refresh_all().observe(ProcessIdentity {
            pid: self.pid,
            start_time_seconds: self.start_time_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn incomplete_snapshot_confirms_only_a_missing_process_exit() {
        let mut child = Command::new("cat").stdin(Stdio::piped()).spawn().unwrap();
        let identity = process_identity(child.id()).unwrap();
        let incomplete = ProcessTable {
            processes: HashMap::new(),
        };
        assert_eq!(incomplete.observe(identity), ProcessObservation::Unknown);
        child.wait().unwrap();
        assert_eq!(incomplete.observe(identity), ProcessObservation::Exited);
    }
}
