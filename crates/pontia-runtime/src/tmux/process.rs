use std::{
    path::Path,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::process::{
    ProcessIdentity, ProcessIdentityObservation, ProcessInfo, ProcessTable,
    system_boot_time_seconds,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TmuxProcessFingerprint {
    pub boot_time_seconds: u64,
    pub tmux_socket_path: String,
    pub tmux_pane_id: String,
    pub pane_pid: u32,
    pub pane_start_time_seconds: u64,
    pub agent_pid: u32,
    pub agent_start_time_seconds: u64,
    pub agent_comm: String,
    pub agent_argv0: Option<String>,
}

pub(crate) fn capture_fingerprint(
    socket_path: &str,
    pane_id: &str,
    process_names: &[&str],
) -> Option<TmuxProcessFingerprint> {
    let pane_pid = pane_pid(socket_path, pane_id)?;
    let first = ProcessTable::refresh_all();
    let pane = first.get(pane_pid)?;
    let (agent, _) = first
        .values()
        .filter(|process| process_matches(process, process_names))
        .filter_map(|process| {
            descendant_depth(&first, process.pid, pane_pid).map(|depth| (process, depth))
        })
        .min_by_key(|(process, depth)| (*depth, process.pid))?;

    let fingerprint = TmuxProcessFingerprint {
        boot_time_seconds: system_boot_time_seconds()?,
        tmux_socket_path: socket_path.into(),
        tmux_pane_id: pane_id.into(),
        pane_pid,
        pane_start_time_seconds: pane.start_time_seconds,
        agent_pid: agent.pid,
        agent_start_time_seconds: agent.start_time_seconds,
        agent_comm: agent.name.clone(),
        agent_argv0: agent.argv0.clone(),
    };

    // Refresh identity and ownership fields so an exit or PID reuse during capture
    // cannot produce a fingerprint assembled from different process instances.
    (observe_fingerprint(&fingerprint) == ProcessObservation::Alive).then_some(fingerprint)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessObservation {
    Alive,
    Exited,
    Unknown,
}

pub(crate) fn observe_fingerprint(fingerprint: &TmuxProcessFingerprint) -> ProcessObservation {
    let Some(boot_time) = system_boot_time_seconds() else {
        return ProcessObservation::Unknown;
    };
    if boot_time != fingerprint.boot_time_seconds {
        return ProcessObservation::Exited;
    }

    let processes = ProcessTable::refresh_all();
    // The agent identity owns liveness. Losing the pane while that process still
    // exists is uncertain ownership, not evidence that the process exited.
    match observe_identity(
        &processes,
        fingerprint.agent_pid,
        fingerprint.agent_start_time_seconds,
    ) {
        ProcessObservation::Alive => {}
        observation => return observation,
    }
    if fingerprint.pane_pid != fingerprint.agent_pid
        && observe_identity(
            &processes,
            fingerprint.pane_pid,
            fingerprint.pane_start_time_seconds,
        ) != ProcessObservation::Alive
    {
        return ProcessObservation::Unknown;
    }
    if pane_pid(&fingerprint.tmux_socket_path, &fingerprint.tmux_pane_id)
        != Some(fingerprint.pane_pid)
    {
        return ProcessObservation::Unknown;
    }
    if descendant_depth(&processes, fingerprint.agent_pid, fingerprint.pane_pid).is_some() {
        ProcessObservation::Alive
    } else {
        ProcessObservation::Unknown
    }
}

fn observe_identity(
    processes: &ProcessTable,
    pid: u32,
    expected_start_time_seconds: u64,
) -> ProcessObservation {
    match processes.observe(ProcessIdentity {
        pid,
        start_time_seconds: expected_start_time_seconds,
    }) {
        ProcessIdentityObservation::Alive => ProcessObservation::Alive,
        ProcessIdentityObservation::Exited => ProcessObservation::Exited,
        ProcessIdentityObservation::Unknown => ProcessObservation::Unknown,
    }
}

fn pane_pid(socket_path: &str, pane_id: &str) -> Option<u32> {
    let output = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_pid}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

fn process_matches(process: &ProcessInfo, process_names: &[&str]) -> bool {
    !matches!(
        process.status,
        sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead
    ) && process_names.iter().any(|expected| {
        process.name == *expected
            || process
                .argv0
                .as_deref()
                .and_then(|argv0| Path::new(argv0.trim_start_matches('-')).file_name())
                .and_then(|name| name.to_str())
                == Some(*expected)
    })
}

fn descendant_depth(
    processes: &ProcessTable,
    candidate_pid: u32,
    ancestor_pid: u32,
) -> Option<usize> {
    let mut pid = candidate_pid;
    // A process tree cannot legitimately contain more ancestors than there are
    // processes. The bound also protects against malformed/cyclic snapshots.
    for depth in 0..=processes.len() {
        if pid == ancestor_pid {
            return Some(depth);
        }
        let process = processes.get(pid)?;
        let parent_pid = process.parent_pid?;
        if parent_pid == 0 || parent_pid == pid {
            return None;
        }
        pid = parent_pid;
    }
    None
}
