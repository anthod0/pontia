use std::process::{Command, Stdio};

use super::pane::is_pane_alive;

const REUSABLE_SHELL_COMMANDS: &[&str] = &["sh", "bash", "zsh", "fish", "dash"];

pub(crate) fn is_reusable_shell_pane(socket_path: &str, pane_id: &str) -> bool {
    if !is_pane_alive(socket_path, pane_id) {
        return false;
    }
    pane_current_command(socket_path, pane_id)
        .as_deref()
        .is_some_and(is_reusable_shell_command)
        && pane_tty(socket_path, pane_id)
            .and_then(|tty| foreground_processes_for_tty(&tty))
            .is_some_and(|ps_output| foreground_command_is_reusable_shell(&ps_output))
}

fn pane_tty(socket_path: &str, pane_id: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_tty}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn foreground_processes_for_tty(tty: &str) -> Option<String> {
    let output = Command::new("ps")
        .args(["-t", tty, "-o", "pid=,stat=,comm="])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn foreground_command_is_reusable_shell(ps_output: &str) -> bool {
    let mut foreground_commands = ps_output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _pid = fields.next()?;
            let stat = fields.next()?;
            let command = fields.next()?;
            stat.contains('+').then_some(command)
        })
        .peekable();

    foreground_commands.peek().is_some() && foreground_commands.all(is_reusable_shell_command)
}

fn is_reusable_shell_command(command: &str) -> bool {
    REUSABLE_SHELL_COMMANDS.contains(&command)
}

fn pane_current_command(socket_path: &str, pane_id: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_current_command}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_command_allows_only_shell_processes() {
        let ps_output = " 1234 Ss+ bash\n 1235 S+ sh\n 1236 S+ zsh\n";

        assert!(foreground_command_is_reusable_shell(ps_output));
    }

    #[test]
    fn foreground_command_rejects_non_shell_processes_even_when_shell_exists() {
        let ps_output = " 1234 Ss bash\n 1235 S+ pi\n";

        assert!(!foreground_command_is_reusable_shell(ps_output));
    }

    #[test]
    fn foreground_command_rejects_empty_or_background_only_processes() {
        assert!(!foreground_command_is_reusable_shell(""));
        assert!(!foreground_command_is_reusable_shell(" 1234 Ss bash\n"));
    }
}
