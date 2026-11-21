use pontia_config::AppConfig;
use serde_json::{Value, json};
use std::{env, path::PathBuf};

/// Shares `paths` as one batch and returns one file ID per line, in path order.
pub async fn run(paths: Vec<PathBuf>, config: &AppConfig) -> Result<String, String> {
    let session_id = env::var("PONTIA_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .ok_or("Missing PONTIA_SESSION_ID; run send-file inside a Pontia Session")?;
    let current = env::current_dir().map_err(|e| e.to_string())?;
    let paths = paths
        .iter()
        .map(|path| {
            let path = current.join(path);
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{}: File path must be UTF-8", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let addr = crate::lifecycle::local_health_addr(config.bind_addr);
    let mut url = reqwest::Url::parse(&format!("http://{addr}/api/v1/sessions/"))
        .map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "Invalid service URL")?
        .pop_if_empty()
        .push(&session_id)
        .push("files");
    let mut request = reqwest::Client::new()
        .post(url)
        .json(&json!({"paths": paths}));
    if let Some(token) = &config.external_api_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Cannot reach pontiad: {e}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid service response: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "{status}: {}",
            body["error"]["message"]
                .as_str()
                .unwrap_or("Could not share files")
        ));
    }
    body["data"]["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .map(|file| file["file_id"].as_str())
                .collect::<Option<Vec<_>>>()
        })
        .map(|ids| ids.join("\n"))
        .ok_or_else(|| "Service response has no file IDs".into())
}
