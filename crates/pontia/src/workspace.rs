use pontia_config::AppConfig;
use serde_json::Value;
use std::{path::Path, time::Duration};

pub async fn is_active(config: &AppConfig, path: &Path) -> Result<bool, String> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    if !path.is_dir() {
        return Ok(false);
    }
    let addr = pontia::lifecycle::local_health_addr(config.bind_addr);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(format!("http://{addr}/api/v1/workspaces"));
    if let Some(token) = &config.external_api_token {
        request = request.bearer_auth(token);
    }
    let body: Value = request
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let workspaces = body["data"]["workspaces"]
        .as_array()
        .ok_or("Service response has no Workspaces")?;
    Ok(workspaces.iter().any(|workspace| {
        workspace["state"] == "active" && workspace["canonical_path"].as_str() == path.to_str()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn checks_canonical_directory_and_active_registration_without_writes() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&project, &link).unwrap();
        for (state, expected) in [("active", true), ("archived", false), ("deleted", false)] {
            let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut config =
                pontia_clients::config_from_vars(&std::collections::HashMap::from([(
                    "PONTIA_HOME".into(),
                    root.path().display().to_string(),
                )]))
                .unwrap();
            config.bind_addr = server.local_addr().unwrap();
            config.external_api_token = Some("fixture-token".into());
            let body = json!({"data":{"workspaces":[{"state":state,"canonical_path":project}]}})
                .to_string();
            let serving = tokio::spawn(async move {
                let (mut connection, _) = server.accept().await.unwrap();
                let mut request = vec![0; 4096];
                let size = connection.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..size]);
                assert!(request.starts_with("GET /api/v1/workspaces "));
                assert!(request.contains("authorization: Bearer fixture-token"));
                connection.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            assert_eq!(is_active(&config, &link).await.unwrap(), expected);
            serving.await.unwrap();
        }
    }
}
