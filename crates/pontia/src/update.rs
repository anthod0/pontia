use pontia_agent_control::queries::UpdateBlocker;
use pontia_config::AppConfig;
use serde::Deserialize;
use std::path::PathBuf;

pub struct PreparedUpdate(pontia_update::PreparedUpdate);

pub async fn prepare(config: Option<&AppConfig>) -> Result<Option<PreparedUpdate>, String> {
    if let Some(config) = config {
        ensure_no_active_execution(config).await?;
    }
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

    pub async fn install(
        self,
        config: Option<&AppConfig>,
        restart: impl FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(config) = config {
            ensure_no_active_execution(config).await?;
        }
        self.0.install(restart)
    }
}

pub async fn ensure_no_active_execution(config: &AppConfig) -> Result<(), String> {
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
            "Update refused: could not check active execution: {error}. No binaries were replaced."
        )
    })?;
    if response.data.blockers.is_empty() {
        return Ok(());
    }
    let blockers = response
        .data
        .blockers
        .iter()
        .map(|blocker| format!("  {}: {}", blocker.session_id, blocker.reason))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "Update refused:\n{blockers}\nFinish or resolve these activities, then run pontia update again. No binaries were replaced."
    ))
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
        let result = ensure_no_active_execution(&config).await;
        server.await.unwrap();
        result
    }

    #[tokio::test]
    async fn active_execution_refuses_update_before_release_preparation() {
        let (config, _root, server) = check_server(
            200,
            r#"{"data":{"blockers":[{"session_id":"session-a","reason":"active"}]}}"#,
        )
        .await;
        let error = prepare(Some(&config)).await.err().unwrap();
        server.await.unwrap();
        assert!(error.contains("session-a"));
    }

    #[tokio::test]
    async fn update_requires_an_explicit_empty_blocker_list() {
        assert!(check(200, r#"{"data":{"blockers":[]}}"#).await.is_ok());
        let error = check(
            200,
            r#"{"data":{"blockers":[{"session_id":"session-a","reason":"active"}]}}"#,
        )
        .await
        .unwrap_err();
        assert!(error.contains("session-a"));
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
        assert!(ensure_no_active_execution(&config).await.is_err());
    }
}
