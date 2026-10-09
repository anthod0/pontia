use std::{collections::HashMap, ffi::OsStr};

#[cfg(target_os = "macos")]
use sysinfo::Signal;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time_seconds: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub start_time_seconds: u64,
    pub status: ProcessStatus,
    pub name: String,
    pub argv0: Option<String>,
}

pub(crate) struct ProcessTable {
    _system: System,
    processes: HashMap<u32, ProcessInfo>,
    query_complete: bool,
}

impl ProcessTable {
    pub(crate) fn refresh_all() -> Self {
        let mut system = System::new();
        let refreshed_processes = system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::Always)
                .without_tasks(),
        );
        let processes: HashMap<u32, ProcessInfo> = system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let pid = pid.as_u32();
                (
                    pid,
                    ProcessInfo {
                        pid,
                        parent_pid: process.parent().map(Pid::as_u32),
                        start_time_seconds: process.start_time(),
                        status: process.status(),
                        name: os_string(process.name()),
                        argv0: process.cmd().first().and_then(|value| {
                            let value = value.to_str()?;
                            (!value.is_empty()).then(|| value.to_string())
                        }),
                    },
                )
            })
            .collect();
        // sysinfo counts PIDs selected for refresh even when reading one PID's
        // details fails on macOS. Only a complete snapshot can prove absence.
        let query_complete = !processes.is_empty() && refreshed_processes == processes.len();
        Self {
            _system: system,
            processes,
            query_complete,
        }
    }

    pub(crate) fn get(&self, pid: u32) -> Option<&ProcessInfo> {
        self.processes.get(&pid)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &ProcessInfo> {
        self.processes.values()
    }

    pub(crate) fn len(&self) -> usize {
        self.processes.len()
    }

    pub(crate) fn observe(&self, identity: ProcessIdentity) -> ProcessIdentityObservation {
        let Some(process) = self.get(identity.pid) else {
            return if self.query_complete {
                ProcessIdentityObservation::Exited
            } else {
                ProcessIdentityObservation::Unknown
            };
        };
        if process.start_time_seconds == 0 || matches!(process.status, ProcessStatus::Unknown(_)) {
            return ProcessIdentityObservation::Unknown;
        }
        if process.start_time_seconds != identity.start_time_seconds
            || matches!(process.status, ProcessStatus::Zombie | ProcessStatus::Dead)
        {
            ProcessIdentityObservation::Exited
        } else {
            ProcessIdentityObservation::Alive
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn terminate(&self, identity: ProcessIdentity) -> TerminateResult {
        match self.observe(identity) {
            ProcessIdentityObservation::Alive => {}
            ProcessIdentityObservation::Exited => return TerminateResult::Exited,
            ProcessIdentityObservation::Unknown => return TerminateResult::Unknown,
        }
        let Some(process) = self._system.process(Pid::from_u32(identity.pid)) else {
            return TerminateResult::Unknown;
        };
        // sysinfo sends by PID on macOS. The process can exit and its PID can be
        // reused after the identity check above but before the signal reaches the
        // kernel; unlike Linux pidfd, this API cannot close that race.
        match process.kill_with(Signal::Term) {
            Some(true) => TerminateResult::Signalled,
            Some(false) | None => TerminateResult::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessIdentityObservation {
    Alive,
    Exited,
    Unknown,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminateResult {
    Signalled,
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
    (table.observe(identity) == ProcessIdentityObservation::Alive).then_some(identity)
}

pub fn system_boot_time_seconds() -> Option<u64> {
    let boot_time = System::boot_time();
    (boot_time != 0).then_some(boot_time)
}

fn os_string(value: &OsStr) -> String {
    value.to_string_lossy().into_owned()
}
