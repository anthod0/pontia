use std::process::{Command, Stdio};

pub struct TmuxSessionGuard {
    tmux_session: String,
}

impl TmuxSessionGuard {
    pub fn for_session(session_id: &str) -> Self {
        let sanitized: String = session_id
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        Self {
            tmux_session: format!("llmparty_{sanitized}"),
        }
    }
}

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.tmux_session])
            .stderr(Stdio::null())
            .status();
    }
}
