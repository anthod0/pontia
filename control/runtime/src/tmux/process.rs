use std::{
    collections::HashMap,
    fs,
    path::Path,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TmuxProcessFingerprint {
    pub boot_id: String,
    pub pane_pid: u32,
    pub pane_start_time_ticks: u64,
    pub agent_pid: u32,
    pub agent_start_time_ticks: u64,
    pub agent_comm: String,
    pub agent_argv0: Option<String>,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    parent_pid: u32,
    start_time_ticks: u64,
    state: char,
    comm: String,
    argv0: Option<String>,
}

pub(crate) fn capture_fingerprint(
    socket_path: &str,
    pane_id: &str,
    process_names: &[&str],
) -> Option<TmuxProcessFingerprint> {
    let pane_pid = pane_pid(socket_path, pane_id)?;
    let first = process_table().ok()?;
    let pane = first.get(&pane_pid)?;
    let (agent, _) = first
        .values()
        .filter(|process| process.state != 'Z' && process_matches(process, process_names))
        .filter_map(|process| {
            descendant_depth(&first, process.pid, pane_pid).map(|depth| (process, depth))
        })
        .min_by_key(|(process, depth)| (*depth, process.pid))?;

    let fingerprint = TmuxProcessFingerprint {
        boot_id: read_boot_id().ok()?,
        pane_pid,
        pane_start_time_ticks: pane.start_time_ticks,
        agent_pid: agent.pid,
        agent_start_time_ticks: agent.start_time_ticks,
        agent_comm: agent.comm.clone(),
        agent_argv0: agent.argv0.clone(),
    };

    // Re-read identity fields so a process exit/PID reuse during capture cannot
    // produce a fingerprint assembled from two different processes.
    validate_fingerprint(socket_path, pane_id, &fingerprint).then_some(fingerprint)
}

pub(crate) fn validate_fingerprint(
    socket_path: &str,
    pane_id: &str,
    fingerprint: &TmuxProcessFingerprint,
) -> bool {
    if read_boot_id().ok().as_deref() != Some(fingerprint.boot_id.as_str()) {
        return false;
    }
    if pane_pid(socket_path, pane_id) != Some(fingerprint.pane_pid) {
        return false;
    }
    let Ok(processes) = process_table() else {
        return false;
    };
    let Some(pane) = processes.get(&fingerprint.pane_pid) else {
        return false;
    };
    if pane.start_time_ticks != fingerprint.pane_start_time_ticks {
        return false;
    }
    let Some(agent) = processes.get(&fingerprint.agent_pid) else {
        return false;
    };
    agent.state != 'Z'
        && agent.start_time_ticks == fingerprint.agent_start_time_ticks
        && descendant_depth(&processes, agent.pid, pane.pid).is_some()
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
    process_names.iter().any(|expected| {
        process.comm == *expected
            || process
                .argv0
                .as_deref()
                .and_then(|argv0| Path::new(argv0.trim_start_matches('-')).file_name())
                .and_then(|name| name.to_str())
                == Some(*expected)
    })
}

fn descendant_depth(
    processes: &HashMap<u32, ProcessInfo>,
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
        let process = processes.get(&pid)?;
        if process.parent_pid == 0 || process.parent_pid == pid {
            return None;
        }
        pid = process.parent_pid;
    }
    None
}

fn process_table() -> std::io::Result<HashMap<u32, ProcessInfo>> {
    let mut processes = HashMap::new();
    for entry in fs::read_dir("/proc")? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        let stat = match fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(_) => continue,
        };
        let Some((parent_pid, start_time_ticks, state)) = parse_stat(&stat) else {
            continue;
        };
        let comm = match fs::read_to_string(entry.path().join("comm")) {
            Ok(comm) => comm.trim_end().to_string(),
            Err(_) => continue,
        };
        let argv0 = fs::read(entry.path().join("cmdline"))
            .ok()
            .and_then(|bytes| bytes.split(|byte| *byte == 0).next().map(Vec::from))
            .filter(|bytes| !bytes.is_empty())
            .and_then(|bytes| String::from_utf8(bytes).ok());
        processes.insert(
            pid,
            ProcessInfo {
                pid,
                parent_pid,
                start_time_ticks,
                state,
                comm,
                argv0,
            },
        );
    }
    Ok(processes)
}

fn parse_stat(stat: &str) -> Option<(u32, u64, char)> {
    let close_paren = stat.rfind(')')?;
    let fields = stat
        .get(close_paren + 1..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    // fields starts at proc(5) field 3 (`state`); starttime is field 22.
    let state = fields.first()?.chars().next()?;
    let parent_pid = fields.get(1)?.parse().ok()?;
    let start_time_ticks = fields.get(19)?.parse().ok()?;
    Some((parent_pid, start_time_ticks, state))
}

fn read_boot_id() -> std::io::Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_stat_with_spaces_and_parentheses_in_comm() {
        let mut fields = vec!["S".to_string(), "42".to_string()];
        fields.extend((5..22).map(|field| field.to_string()));
        fields.push("987654".to_string());
        let stat = format!("123 (agent (worker)) {}", fields.join(" "));

        assert_eq!(parse_stat(&stat), Some((42, 987654, 'S')));
    }

    #[test]
    fn descendant_depth_accepts_root_and_descendants() {
        let process = |pid, parent_pid| ProcessInfo {
            pid,
            parent_pid,
            start_time_ticks: 1,
            state: 'S',
            comm: String::new(),
            argv0: None,
        };
        let processes = HashMap::from([
            (10, process(10, 1)),
            (11, process(11, 10)),
            (12, process(12, 11)),
        ]);

        assert_eq!(descendant_depth(&processes, 10, 10), Some(0));
        assert_eq!(descendant_depth(&processes, 12, 10), Some(2));
        assert_eq!(descendant_depth(&processes, 10, 12), None);
    }
}
