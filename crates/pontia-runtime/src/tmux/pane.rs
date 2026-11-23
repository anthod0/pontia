use std::process::{Command, Stdio};

use pontia_core::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxPaneBinding {
    pub socket_path: String,
    pub pane_id: String,
}

pub(crate) fn run_launch_command_in_pane(
    socket_path: &str,
    pane_id: &str,
    launch_command: &str,
) -> Result<()> {
    if !is_pane_alive(socket_path, pane_id) {
        return Err(Error::Domain(format!(
            "tmux runtime pane {pane_id} is not alive"
        )));
    }
    let status = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "send-keys",
            "-t",
            pane_id,
            launch_command,
            "Enter",
        ])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux pane script start failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux pane script start failed with status {status}"
        )));
    }
    Ok(())
}

pub(crate) fn kill_pane(socket_path: &str, pane_id: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["-S", socket_path, "kill-pane", "-t", pane_id])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux kill-pane failed: {err}")))?;
    if status.success() || !is_pane_alive(socket_path, pane_id) {
        Ok(())
    } else {
        Err(Error::Domain(format!(
            "tmux kill-pane failed with status {status}"
        )))
    }
}

pub(crate) fn is_pane_alive(socket_path: &str, pane_id: &str) -> bool {
    let output = Command::new("tmux")
        .args(["-S", socket_path, "list-panes", "-a", "-F", "#{pane_id}"])
        .stderr(Stdio::null())
        .output();
    output.is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line == pane_id)
    })
}

pub fn pane_binding(runtime_handle: &str) -> Option<TmuxPaneBinding> {
    let output = Command::new("tmux")
        .args([
            // Without UTF-8 output, tmux replaces the tab separator in the C locale.
            "-u",
            "display-message",
            "-p",
            "-t",
            runtime_handle,
            "#{socket_path}\t#{pane_id}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let mut parts = text.trim().split('\t');
    let socket_path = parts.next()?.to_string();
    let pane_id = parts.next()?.to_string();
    if socket_path.is_empty() || pane_id.is_empty() {
        return None;
    }
    Some(TmuxPaneBinding {
        socket_path,
        pane_id,
    })
}

/// Pane numbers are reused when a tmux server restarts. Keep the launch
/// marker and process tree alongside the number when inspecting old records.
pub struct PaneProcess {
    pid: u32,
    runtime: String,
    start_command: String,
}

impl PaneProcess {
    pub fn owner(
        &self,
        runtime: &str,
        agent: Option<&crate::ProcessFingerprint>,
        launch_log: &std::path::Path,
        session: &str,
    ) -> Result<Option<crate::ProcessFingerprint>> {
        let owned = if self.runtime.is_empty() {
            let verified = agent.is_some_and(|p| p.is_in_process_tree(self.pid));
            if !verified && self.matches_launch_log(launch_log, session)? {
                return Err(Error::ControlUnknown(format!(
                    "cannot verify the process in an older tmux pane for Session {session} (pane PID {}). Inspect the pane before stopping it manually",
                    self.pid
                )));
            }
            verified
        } else {
            self.runtime == runtime
        };
        if owned {
            Ok(crate::ProcessFingerprint::capture(self.pid))
        } else {
            Ok(None)
        }
    }

    // An older pane may still be waiting for its first agent registration.
    // Its start command names the unique script logged by Pontia at launch.
    fn matches_launch_log(&self, log: &std::path::Path, session: &str) -> Result<bool> {
        use std::io::BufRead;
        let Some((_, suffix)) = self.start_command.split_once("/state/launch/launch_evt_") else {
            return Ok(false);
        };
        let Some((id, _)) = suffix.split_once(".sh") else {
            return Ok(false);
        };
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Ok(false);
        }
        let expected = format!("session={session} launch=launch_evt_{id} pontia runtime started");
        let file = match std::fs::File::open(log) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        for line in std::io::BufReader::new(file).split(b'\n') {
            if line? == expected.as_bytes() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub fn pane_processes(socket: &str) -> Result<std::collections::HashMap<String, PaneProcess>> {
    let mut panes = std::collections::HashMap::new();
    if !std::path::Path::new(socket).try_exists()? {
        return Ok(panes);
    }
    let output = Command::new("tmux")
        .args([
            "-S",
            socket,
            "list-panes",
            "-a",
            "-F",
            "#{pane_id} #{pane_pid} #{@pontia_runtime} #{pane_start_command}",
        ])
        .output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if message.contains("no server running")
            || message.contains("Connection refused")
            || message.contains("No such file or directory")
        {
            return Ok(panes);
        }
        return Err(Error::ControlUnknown(format!(
            "cannot inspect tmux socket {socket}: {message}"
        )));
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.splitn(4, ' ');
        let id = parts.next().unwrap_or_default();
        let pid = parts
            .next()
            .and_then(|pid| pid.parse().ok())
            .ok_or_else(|| Error::ControlUnknown("tmux returned an invalid pane PID".into()))?;
        panes.insert(
            id.into(),
            PaneProcess {
                pid,
                runtime: parts.next().unwrap_or_default().into(),
                start_command: parts.next().unwrap_or_default().into(),
            },
        );
    }
    Ok(panes)
}

pub fn kill_owned_pane(socket: &str, pane: &str, owner: &crate::ProcessFingerprint) -> Result<()> {
    if !pane
        .strip_prefix('%')
        .is_some_and(|n| n.parse::<u32>().is_ok())
    {
        return Err(Error::Domain("invalid tmux pane ID".into()));
    }
    match owner.observe() {
        crate::ProcessObservation::Exited => return Ok(()),
        crate::ProcessObservation::Unknown => {
            return Err(Error::ControlUnknown(
                "cannot verify tmux pane identity".into(),
            ));
        }
        crate::ProcessObservation::Alive => {}
    }
    // tmux checks and removes the target on one server connection.
    let output = Command::new("tmux")
        .args([
            "-S",
            socket,
            "if-shell",
            "-F",
            "-t",
            pane,
            &format!("#{{==:#{{pane_pid}},{}}}", owner.pid),
            &format!("kill-pane -t {pane}"),
        ])
        .output()?;
    if !output.status.success() && owner.observe() != crate::ProcessObservation::Exited {
        return Err(Error::ControlUnknown(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    Ok(())
}
