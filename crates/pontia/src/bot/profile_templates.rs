use std::{fmt::Write, time::Duration};

use reqwest::{Client, header::CONTENT_TYPE};
use serde::Deserialize;

const EMBEDDED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/agent-profile-templates.json"
));
const BODY_LIMIT: usize = 1024 * 1024;

#[derive(Deserialize)]
struct TemplatesDocument {
    schema_version: u32,
    templates: Vec<ProfileTemplate>,
}

#[derive(Deserialize)]
struct ProfileTemplate {
    id: String,
    #[serde(rename = "icon")]
    _icon: Option<String>,
    client_type: String,
    client_key: Option<String>,
    model: String,
    reasoning_effort: Option<String>,
    use_yolo: bool,
    text: TemplateTextMap,
}

#[derive(Deserialize)]
struct TemplateTextMap {
    en: TemplateText,
}

#[derive(Deserialize)]
struct TemplateText {
    name: String,
    description: String,
    system_prompt: String,
}

impl TemplatesDocument {
    fn markdown(&self) -> String {
        let mut result = String::from(
            "# Agent Profile Templates\n\nUse these templates as starting points when creating shared Agent Profiles with `pontia bot profile create`.\nCheck available clients and models with `pontia bot profile clients` and `pontia bot profile models --client TYPE/KEY`, then customize fields for the user's requirements or keep template defaults when unspecified.\n",
        );
        for template in &self.templates {
            let TemplateText {
                name,
                description,
                system_prompt,
            } = &template.text.en;
            let id = &template.id;
            let key = template.client_key.as_deref().unwrap_or("default");
            let longest = system_prompt
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0);
            let fence = "`".repeat(3.max(longest + 1));
            writeln!(
                result,
                "\n## {name} (`{id}`)\n\n- Profile ID: `{id}`\n- Client: `{}/{key}`\n- Model: `{}`",
                template.client_type, template.model
            )
            .unwrap();
            if let Some(effort) = template.reasoning_effort.as_ref().filter(|s| !s.is_empty()) {
                writeln!(result, "- Reasoning effort: `{effort}`").unwrap();
            }
            writeln!(result, "- YOLO mode: `{}`\n\n### Description\n\n{description}\n\n### System prompt\n\n{fence}text\n{system_prompt}\n{fence}", template.use_yolo).unwrap();
        }
        result
    }
}

async fn fetch(client: &Client, origin: &str, json: bool) -> Option<String> {
    let ext = if json { "json" } else { "md" };
    let url = format!(
        "{}/agent-profile-templates.{ext}",
        origin.trim_end_matches('/')
    );
    let mut response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    if !json && let Some(value) = response.headers().get(CONTENT_TYPE) {
        let mime = value.to_str().ok()?.split(';').next()?.trim();
        if !mime.eq_ignore_ascii_case("text/markdown") && !mime.eq_ignore_ascii_case("text/plain") {
            return None;
        }
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > BODY_LIMIT - bytes.len() {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8(bytes).ok()?;
    if json {
        let doc: TemplatesDocument = serde_json::from_str(&body).ok()?;
        if doc.schema_version != 1 || doc.templates.is_empty() {
            return None;
        }
    } else if !body.trim().starts_with("# ") {
        return None;
    }
    Some(body)
}

pub(super) async fn load(origin: &str, json: bool) -> String {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build();
    if let Ok(client) = client
        && let Some(body) = fetch(&client, origin, json).await
    {
        return body;
    }
    if json {
        EMBEDDED.to_owned()
    } else {
        serde_json::from_str::<TemplatesDocument>(EMBEDDED)
            .expect("valid embedded profile templates")
            .markdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, http::StatusCode, routing::get};
    use std::collections::HashSet;

    const FENCED: &str = include_str!("../../tests/fixtures/profile-templates/fenced.json");

    #[test]
    fn embedded_catalog_has_unique_ids_and_supported_schema() {
        let doc: TemplatesDocument = serde_json::from_str(EMBEDDED).unwrap();
        assert_eq!(doc.schema_version, 1);
        assert!(!doc.templates.is_empty());
        let mut ids = HashSet::new();
        assert!(doc.templates.iter().all(|t| ids.insert(&t.id)));
    }

    #[test]
    fn renders_the_shared_markdown_fixture() {
        let doc: TemplatesDocument = serde_json::from_str(FENCED).unwrap();
        assert_eq!(
            doc.markdown(),
            include_str!("../../tests/fixtures/profile-templates/fenced.md")
        );
    }

    #[tokio::test]
    async fn validates_published_responses_before_using_them() {
        let future_json = FENCED.replace("\"schema_version\"", "\"extra\":true,\"schema_version\"");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route(
                "/agent-profile-templates.md",
                get(|| async { ([(CONTENT_TYPE, "text/markdown")], "# Published\n") }),
            )
            .route(
                "/agent-profile-templates.json",
                get({
                    let body = future_json.clone();
                    move || async { body }
                }),
            )
            .route(
                "/html/agent-profile-templates.md",
                get(|| async { axum::response::Html("# Gateway") }),
            )
            .route(
                "/large/agent-profile-templates.md",
                get(|| async { format!("# {}", "a".repeat(BODY_LIMIT)) }),
            )
            .route(
                "/invalid/agent-profile-templates.json",
                get(|| async { "{" }),
            )
            .route(
                "/version/agent-profile-templates.json",
                get(|| async { FENCED.replace("\"schema_version\": 1", "\"schema_version\": 2") }),
            )
            .route(
                "/error/agent-profile-templates.md",
                get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "# Error") }),
            )
            .route(
                "/slow/agent-profile-templates.md",
                get(|| async {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    "# Late"
                }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        assert_eq!(load(&origin, false).await, "# Published\n");
        assert_eq!(load(&origin, true).await, future_json);
        let fallback = serde_json::from_str::<TemplatesDocument>(EMBEDDED)
            .unwrap()
            .markdown();
        for path in ["html", "large", "missing", "error", "slow"] {
            assert_eq!(load(&format!("{origin}/{path}"), false).await, fallback);
        }
        for path in ["invalid", "version", "missing"] {
            assert_eq!(load(&format!("{origin}/{path}"), true).await, EMBEDDED);
        }
        server.abort();
    }
}
