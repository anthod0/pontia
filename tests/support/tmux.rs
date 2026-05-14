use std::process::{Command, Stdio};

pub struct TmuxSessionGuard {
    session_id: String,
    legacy_tmux_session: String,
    short_session_id: String,
}

impl TmuxSessionGuard {
    pub fn for_session(session_id: &str) -> Self {
        let sanitized: String = session_id
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        let id_body = session_id.rsplit('_').next().unwrap_or(session_id);
        let mut chars: Vec<char> = id_body.chars().rev().take(8).collect();
        chars.reverse();
        Self {
            session_id: session_id.to_string(),
            legacy_tmux_session: format!("llmparty_{sanitized}"),
            short_session_id: chars.into_iter().collect(),
        }
    }
}

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.legacy_tmux_session])
            .stderr(Stdio::null())
            .status();

        let Ok(output) = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .stderr(Stdio::null())
            .output()
        else {
            return;
        };
        let sessions = String::from_utf8_lossy(&output.stdout);
        for tmux_session in sessions.lines().filter(|name| {
            name.starts_with("llmparty_")
                && (name.ends_with(&format!("_{}", self.short_session_id))
                    || name.contains(&self.session_id))
        }) {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", tmux_session])
                .stderr(Stdio::null())
                .status();
        }
    }
}
