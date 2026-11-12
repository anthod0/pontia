use super::CodexService;
use crate::runtime::protocol::Connection;
use pontia_core::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const SESSION_ID: &str = "PONTIA_SESSION_ID";

#[derive(Deserialize)]
struct Configuration {
    shell_environment_policy: Option<ShellEnvironmentPolicy>,
}

#[derive(Default, Deserialize)]
struct ShellEnvironmentPolicy {
    include_only: Option<Vec<String>>,
    filters: Option<BTreeMap<String, Filter>>,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Filter {
    Include,
    Exclude,
}

impl CodexService {
    pub(crate) async fn session_config(
        &self,
        session: &str,
        connection: &Connection,
        cwd: &str,
    ) -> Result<Value> {
        let response = connection
            .call("config/read", json!({"cwd":cwd,"includeLayers":false}))
            .await?;
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_environment') FROM sessions WHERE session_id=?",
        )
        .bind(session)
        .fetch_one(&self.pool)
        .await?;
        let environment: BTreeMap<String, String> = stored
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default();
        let mut config = environment_config(session, &response["config"], environment)?;
        if let Some(profile) = self.profiles().codex_binding(session).await?
            && let Some(prompt) = profile.system_prompt
        {
            let existing = profile
                .native_developer_instructions
                .as_deref()
                .or_else(|| response["config"]["developer_instructions"].as_str());
            config["developer_instructions"] =
                json!(super::profile::append_instructions(existing, &prompt));
        }
        Ok(config)
    }
}

pub(crate) fn environment_config(
    session: &str,
    configuration: &Value,
    mut environment: BTreeMap<String, String>,
) -> Result<Value> {
    let configuration: Configuration = serde_json::from_value(configuration.clone())?;
    let policy = configuration.shell_environment_policy.unwrap_or_default();
    environment.insert(SESSION_ID.into(), session.into());
    // Codex merges this table with native settings, retaining their other environment entries.
    let mut config = json!({"shell_environment_policy.set":environment});
    if let Some(mut filters) = policy.filters {
        if filters.values().any(|filter| *filter == Filter::Include) {
            filters.retain(|key, _| !key.eq_ignore_ascii_case(SESSION_ID));
            filters.insert(SESSION_ID.into(), Filter::Include);
            config["shell_environment_policy.filters"] = json!(filters);
        }
    } else if let Some(mut include) = policy.include_only
        && !include.is_empty()
    {
        if !include
            .iter()
            .any(|key| key.eq_ignore_ascii_case(SESSION_ID))
        {
            include.push(SESSION_ID.into());
        }
        config["shell_environment_policy.include_only"] = json!(include);
    }
    Ok(config)
}
