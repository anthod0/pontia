use pontia_config::AppConfig;
use serde_json::{Value, json};
use std::{env, path::PathBuf};

pub async fn run(path: PathBuf, config: &AppConfig) -> Result<String, String> {
    let session_id = env::var("PONTIA_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .ok_or("Missing PONTIA_SESSION_ID; run send-file inside a Pontia Session")?;
    let path = env::current_dir().map_err(|e| e.to_string())?.join(path);
    let path = path.to_str().ok_or("File path must be UTF-8")?;
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
        .json(&json!({"path": path}));
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
                .unwrap_or("Could not share file")
        ));
    }
    body["data"]["file"]["file_id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Service response has no file ID".into())
}
