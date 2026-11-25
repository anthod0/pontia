//! Device-code login commands that print a verification URL and user code.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::Duration,
};

use pontia_agent_control::{
    client_auth::{sanitize_output, strip_ansi},
    client_contract::auth::{
        AuthErrorCode, AuthEventSink, AuthFlowHandle, AuthFlowUpdate, AuthSessionState, AuthStep,
    },
};
use pontia_core::Result;
use pontia_runtime::auth_process::{ProcessGroup, group_command};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Child,
    sync::{mpsc, oneshot},
};

const TERMINATE_GRACE: Duration = Duration::from_secs(2);
const OUTPUT_DRAIN: Duration = Duration::from_millis(500);
const MAX_LINE: usize = 4096;
const TAIL_LINES: usize = 3;
const URL_KEYWORDS: &[&str] = &[
    "device",
    "verify",
    "activate",
    "awsapps.com",
    "x.ai",
    "kiro",
    "code",
];

pub(super) struct DeviceLogin {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub vars: HashMap<String, String>,
    pub expires_in_secs: Option<u64>,
    pub parse_timeout: Duration,
}

/// Start the command in its own process group and report its progress.
pub(super) fn start(login: DeviceLogin, sink: AuthEventSink) -> Result<AuthFlowHandle> {
    let mut command = tokio::process::Command::from(group_command(&login.program));
    command
        .args(&login.args)
        .env_clear()
        .envs(&login.vars)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("BROWSER", "/bin/false")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let group = child.id().and_then(ProcessGroup::adopt);
    let (lines, received) = mpsc::channel(64);
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(read_lines(stdout, lines.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(read_lines(stderr, lines));
    }
    let (cancel, cancelled) = oneshot::channel();
    let flow = tokio::spawn(run(
        Flow {
            child,
            group,
            expires_in_secs: login.expires_in_secs,
            parse_timeout: login.parse_timeout,
        },
        received,
        cancelled,
        sink,
    ));
    Ok(AuthFlowHandle {
        input: None,
        cancel: Box::new(move || {
            Box::pin(async move {
                let _ = cancel.send(());
                let _ = flow.await;
                Ok(())
            })
        }),
    })
}

struct Flow {
    child: Child,
    group: Option<ProcessGroup>,
    expires_in_secs: Option<u64>,
    parse_timeout: Duration,
}

impl Flow {
    async fn stop(&mut self) {
        match self.group.clone() {
            Some(group) => {
                let terminate =
                    tokio::task::spawn_blocking(move || group.terminate(TERMINATE_GRACE));
                let _ = tokio::join!(terminate, self.child.wait());
            }
            None => {
                let _ = self.child.start_kill();
                let _ = self.child.wait().await;
            }
        }
    }
}

/// Dropping the cancellation sender also stops the command.
async fn run(
    mut flow: Flow,
    mut lines: mpsc::Receiver<String>,
    mut cancelled: oneshot::Receiver<()>,
    sink: AuthEventSink,
) {
    let mut parser = DeviceCodeParser::default();
    let mut tail = VecDeque::with_capacity(TAIL_LINES);
    let mut announced = false;
    let mut output_open = true;
    let parse_deadline = tokio::time::sleep(flow.parse_timeout);
    tokio::pin!(parse_deadline);
    let status = loop {
        tokio::select! {
            line = lines.recv(), if output_open => match line {
                Some(line) => {
                    remember(&mut tail, &line);
                    if announced {
                        continue;
                    }
                    let Some((verification_url, user_code)) = parser.feed(&line) else {
                        continue;
                    };
                    announced = true;
                    let step = AuthStep::DeviceCode {
                        verification_url,
                        user_code,
                        expires_in_secs: flow.expires_in_secs,
                        message: None,
                    };
                    let update = AuthFlowUpdate::Step {
                        state: AuthSessionState::AwaitingUser,
                        step,
                        expires_in_secs: flow.expires_in_secs,
                    };
                    if !sink.emit(update).await {
                        return flow.stop().await;
                    }
                }
                None => output_open = false,
            },
            status = flow.child.wait() => break status,
            () = &mut parse_deadline, if !announced => {
                flow.stop().await;
                sink.emit(AuthFlowUpdate::Failed {
                    code: AuthErrorCode::ParseFailed,
                    message: "The login command did not print a device code".into(),
                    retryable: true,
                })
                .await;
                return;
            }
            _ = &mut cancelled => return flow.stop().await,
        }
    };
    if output_open {
        let _ = tokio::time::timeout(OUTPUT_DRAIN, async {
            while let Some(line) = lines.recv().await {
                remember(&mut tail, &line);
            }
        })
        .await;
    }
    sink.emit(exit_update(status, &tail)).await;
}

fn exit_update(status: std::io::Result<ExitStatus>, tail: &VecDeque<String>) -> AuthFlowUpdate {
    match status {
        Ok(status) if status.success() => AuthFlowUpdate::Completed,
        status => {
            let output = tail.iter().cloned().collect::<Vec<_>>().join("\n");
            let message = if output.is_empty() {
                match status {
                    Ok(status) => format!("The login command exited with {status}"),
                    Err(error) => format!("The login command failed: {error}"),
                }
            } else {
                output
            };
            AuthFlowUpdate::Failed {
                code: AuthErrorCode::ProviderRejected,
                message: sanitize_output(&message),
                retryable: true,
            }
        }
    }
}

fn remember(tail: &mut VecDeque<String>, line: &str) {
    if tail.len() == TAIL_LINES {
        tail.pop_front();
    }
    tail.push_back(line.to_owned());
}

/// Split output on newlines and carriage returns, as progress spinners
/// redraw a line without ending it.
async fn read_lines(mut output: impl AsyncRead + Unpin, lines: mpsc::Sender<String>) {
    let mut pending = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let read = match output.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for &byte in &buffer[..read] {
            if byte == b'\n' || byte == b'\r' || pending.len() >= MAX_LINE {
                if !send_line(&lines, &mut pending).await {
                    return;
                }
                if byte == b'\n' || byte == b'\r' {
                    continue;
                }
            }
            pending.push(byte);
        }
    }
    send_line(&lines, &mut pending).await;
}

async fn send_line(lines: &mpsc::Sender<String>, pending: &mut Vec<u8>) -> bool {
    let line = strip_ansi(&String::from_utf8_lossy(pending));
    pending.clear();
    let line = line.trim();
    line.is_empty() || lines.send(line.to_owned()).await.is_ok()
}

#[derive(Default)]
pub(super) struct DeviceCodeParser {
    url: Option<(bool, String)>,
    code: Option<String>,
}

impl DeviceCodeParser {
    /// The verification URL and user code once both have been printed.
    pub fn feed(&mut self, line: &str) -> Option<(String, String)> {
        for url in urls(line) {
            let preferred = URL_KEYWORDS.iter().any(|keyword| url.contains(keyword));
            if self
                .url
                .as_ref()
                .is_none_or(|(known, _)| preferred && !known)
            {
                self.url = Some((preferred, url.to_owned()));
            }
            if self.code.is_none() {
                self.code = query_parameter(url, "user_code").filter(|code| is_code(code, true));
            }
        }
        if self.code.is_none() {
            self.code = code_in_text(line);
        }
        match (&self.url, &self.code) {
            (Some((_, url)), Some(code)) => Some((url.clone(), code.clone())),
            _ => None,
        }
    }
}

fn urls(line: &str) -> impl Iterator<Item = &str> {
    line.match_indices("https://").filter_map(|(start, _)| {
        let rest = &line[start..];
        let end = rest
            .find(|char: char| char.is_whitespace() || matches!(char, '"' | '\'' | '<' | '>'))
            .unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches(['.', ',', ';', ':', ')', ']']);
        (url.len() > "https://".len()).then_some(url)
    })
}

fn query_parameter(url: &str, name: &str) -> Option<String> {
    url.match_indices(name).find_map(|(start, _)| {
        let before = url[..start].chars().next_back()?;
        let value = url[start + name.len()..].strip_prefix('=')?;
        matches!(before, '?' | '&').then(|| {
            let end = value.find(['&', '#']).unwrap_or(value.len());
            value[..end].to_owned()
        })
    })
}

fn is_code(token: &str, allow_undashed: bool) -> bool {
    let valid = |part: &str, range: std::ops::RangeInclusive<usize>| {
        range.contains(&part.len())
            && part
                .chars()
                .all(|char| char.is_ascii_uppercase() || char.is_ascii_digit())
    };
    match token.split_once('-') {
        Some((first, second)) => valid(first, 4..=8) && valid(second, 4..=8),
        None => allow_undashed && valid(token, 6..=10),
    }
}

fn code_in_text(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if is_code(trimmed, false) {
        return Some(trimmed.to_owned());
    }
    let tokens: Vec<&str> = line
        .split(|char: char| !(char.is_ascii_alphanumeric() || char == '-'))
        .filter(|token| !token.is_empty())
        .collect();
    tokens.iter().enumerate().find_map(|(index, token)| {
        let lower = token.to_ascii_lowercase();
        if !(lower.contains("code") || lower == "enter") {
            return None;
        }
        tokens[index + 1..]
            .iter()
            .take(3)
            .find(|candidate| is_code(candidate, true))
            .map(|code| (*code).to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(output: &str) -> Option<(String, String)> {
        let mut parser = DeviceCodeParser::default();
        output.lines().find_map(|line| parser.feed(line))
    }

    #[test]
    fn parses_kiro_output() {
        let output = "\nConfirm the following code in the browser\nCode: CLFN-FPLB\n\nOpen this URL: https://view.awsapps.com/start/#/device?user_code=CLFN-FPLB\n";
        assert_eq!(
            parse(output),
            Some((
                "https://view.awsapps.com/start/#/device?user_code=CLFN-FPLB".into(),
                "CLFN-FPLB".into()
            ))
        );
    }

    #[test]
    fn parses_grok_output() {
        let output = "To sign in, open this URL in your browser:\n\n  https://accounts.x.ai/oauth2/device?user_code=4BJN-SZG3\n\nConfirm this code in your browser:\n\n  4BJN-SZG3\n";
        assert_eq!(
            parse(output),
            Some((
                "https://accounts.x.ai/oauth2/device?user_code=4BJN-SZG3".into(),
                "4BJN-SZG3".into()
            ))
        );
    }

    #[test]
    fn parses_codes_printed_apart_from_the_url() {
        assert_eq!(
            parse("Visit https://example.test/activate.\nEnter the code: QWERTY12"),
            Some(("https://example.test/activate".into(), "QWERTY12".into()))
        );
        assert_eq!(
            parse("Go to https://example.test/home\nhttps://example.test/device\nABCD-1234"),
            Some(("https://example.test/device".into(), "ABCD-1234".into()))
        );
        assert_eq!(parse("Confirm this code in your browser"), None);
        assert_eq!(parse("https://example.test/device only"), None);
    }

    #[tokio::test]
    async fn spinner_redraws_and_escapes_are_split_into_clean_lines() {
        let (sender, mut lines) = mpsc::channel(16);
        let output: &[u8] = b"\x1b[?25l\r\xe2\x96\xb0 Logging in...\r\xe2\x96\xb0 Logging in...\nCode: \x1b[1mAB12-CD34\x1b[0m\n";
        read_lines(output, sender).await;
        let mut received = Vec::new();
        while let Some(line) = lines.recv().await {
            received.push(line);
        }
        assert_eq!(
            received,
            [
                "\u{25b0} Logging in...",
                "\u{25b0} Logging in...",
                "Code: AB12-CD34"
            ]
        );
    }
}
