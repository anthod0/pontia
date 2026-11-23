use pontia_agent_control::queries::UpdateBlocker;
use pontia_config::AppConfig;
use serde::Deserialize;
use std::path::PathBuf;

pub struct PreparedUpdate(pontia_update::PreparedUpdate);

pub async fn prepare() -> Result<Option<PreparedUpdate>, String> {
    pontia_update::prepare(
        "pontia",
        &["pontia", "pontiad"],
        Some(pontia_version::version()),
    )
    .await
    .map(|update| update.map(PreparedUpdate))
}

impl PreparedUpdate {
    pub fn daemon_path(&self) -> PathBuf {
        self.0.directory().join("pontiad")
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn ensure_no_unmanaged_daemon(&self) -> Result<(), String> {
        pontia_update::ensure_not_running(&self.daemon_path())
    }

    pub fn install(self, restart: impl FnMut() -> Result<(), String>) -> Result<(), String> {
        self.0.install(restart)
    }
}

pub async fn ensure_no_active_execution(config: &AppConfig, operation: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Response {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        blockers: Vec<UpdateBlocker>,
    }

    let addr = pontia::lifecycle::local_health_addr(config.bind_addr);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(format!("http://{addr}/api/v1/update/blockers"));
    if let Some(token) = &config.external_api_token {
        request = request.bearer_auth(token);
    }
    let check = async {
        request
            .send()
            .await?
            .error_for_status()?
            .json::<Response>()
            .await
    };
    let response = check.await.map_err(|error| {
        format!(
            "{operation} refused: could not check active execution: {error}. No changes were made."
        )
    })?;
    if response.data.blockers.is_empty() {
        return Ok(());
    }
    let home = std::env::var("HOME").ok();
    Err(refusal(operation, &response.data.blockers, home.as_deref()))
}

const LISTED_SESSIONS: usize = 10;
const TITLE_CHARS: usize = 60;

fn refusal(operation: &str, blockers: &[UpdateBlocker], home: Option<&str>) -> String {
    let mut sessions: Vec<(&UpdateBlocker, Vec<&str>)> = Vec::new();
    for blocker in blockers {
        match sessions
            .iter_mut()
            .find(|(session, _)| session.session_id == blocker.session_id)
        {
            Some((_, reasons)) => reasons.push(&blocker.reason),
            None => sessions.push((blocker, vec![&blocker.reason])),
        }
    }
    let count = sessions.len();
    let mut lines = vec![format!(
        "{operation} refused: {count} {} still working:",
        if count == 1 {
            "Session is"
        } else {
            "Sessions are"
        }
    )];
    for (session, reasons) in sessions.iter().take(LISTED_SESSIONS) {
        let title = match session.title.as_deref().map(str::trim) {
            Some(title) if !title.is_empty() => title_line(title),
            _ => "Untitled Session".into(),
        };
        let place = match &session.workspace_path {
            Some(path) => format!("{}, {}", session.client_type, home_relative(path, home)),
            None => session.client_type.clone(),
        };
        lines.push(format!("  - {title} ({place}): {}", reasons.join(", ")));
    }
    if count > LISTED_SESSIONS {
        lines.push(format!("  and {} more", count - LISTED_SESSIONS));
    }
    lines.push(format!(
        "Wait for these Sessions to finish or respond to them in the Dashboard, then run pontia {operation} again. No changes were made."
    ));
    lines.join("\n")
}

fn title_line(title: &str) -> String {
    let line = title.lines().next().unwrap_or_default();
    if line.chars().count() > TITLE_CHARS || line.len() < title.len() {
        let short: String = line.chars().take(TITLE_CHARS).collect();
        format!("\"{}...\"", short.trim_end())
    } else {
        format!("\"{line}\"")
    }
}

fn home_relative(path: &str, home: Option<&str>) -> String {
    match home.and_then(|home| path.strip_prefix(home)) {
        Some("") => "~".into(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn check_server(
        status: u16,
        body: &'static str,
    ) -> (AppConfig, tempfile::TempDir, tokio::task::JoinHandle<()>) {
        let root = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = pontia_clients::config_from_vars(&HashMap::from([(
            "PONTIA_HOME".into(),
            root.path().display().to_string(),
        )]))
        .unwrap();
        config.bind_addr = listener.local_addr().unwrap();
        config.external_api_token = Some("update-test-token".into());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get /api/v1/update/blockers http/1.1\r\n"));
            assert!(request.contains("authorization: bearer update-test-token\r\n"));
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        (config, root, server)
    }

    async fn check(status: u16, body: &'static str) -> Result<(), String> {
        let (config, _root, server) = check_server(status, body).await;
        let result = ensure_no_active_execution(&config, "update").await;
        server.await.unwrap();
        result
    }

    #[tokio::test]
    async fn update_requires_an_explicit_empty_blocker_list() {
        assert!(check(200, r#"{"data":{"blockers":[]}}"#).await.is_ok());
        let error = check(
            200,
            r#"{"data":{"blockers":[{"session_id":"session-a","title":"Fix login","client_type":"codex","workspace_path":null,"reason":"running a task"}]}}"#,
        )
        .await
        .unwrap_err();
        assert!(error.contains("Fix login"));
    }

    #[tokio::test]
    async fn unavailable_or_invalid_execution_checks_refuse_updates() {
        for (status, body) in [
            (401, r#"{"data":{"blockers":[]}}"#),
            (404, r#"{"data":{"blockers":[]}}"#),
            (500, r#"{"data":{"blockers":[]}}"#),
            (200, "not json"),
            (200, r#"{"data":{}}"#),
            (200, r#"{"data":{"blockers":[{}]}}"#),
        ] {
            assert!(check(status, body).await.is_err());
        }
    }

    fn blocker(session: &str, reason: &str) -> UpdateBlocker {
        UpdateBlocker {
            session_id: session.into(),
            title: Some(format!("Task {session}")),
            client_type: "pi".into(),
            workspace_path: Some("/home/alice/project".into()),
            reason: reason.into(),
        }
    }

    #[test]
    fn refusal_lists_each_session_once_and_caps_the_list() {
        let mut blockers = vec![
            blocker("s0", "running a task"),
            blocker("s0", "has messages not yet delivered"),
        ];
        blockers.extend((1..12).map(|index| blocker(&format!("s{index}"), "running a task")));
        let message = refusal("update", &blockers, Some("/home/alice"));
        assert_eq!(message.matches("Task s0").count(), 1);
        assert!(message.contains("has messages not yet delivered"));
        assert!(message.contains("Task s9"));
        assert!(!message.contains("Task s10"));
    }

    #[tokio::test]
    async fn a_stopped_service_does_not_pass_the_execution_check() {
        let root = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = pontia_clients::config_from_vars(&HashMap::from([(
            "PONTIA_HOME".into(),
            root.path().display().to_string(),
        )]))
        .unwrap();
        config.bind_addr = listener.local_addr().unwrap();
        drop(listener);
        assert!(ensure_no_active_execution(&config, "update").await.is_err());
    }
}
