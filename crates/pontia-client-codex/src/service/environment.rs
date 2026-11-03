use super::CodexService;
use crate::runtime::protocol::Connection;
use pontia_core::{Error, Result};
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
        let configuration: Configuration = serde_json::from_value(response["config"].clone())?;
        let policy = configuration.shell_environment_policy.unwrap_or_default();
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_environment') FROM sessions WHERE session_id=?",
        )
        .bind(session)
        .fetch_one(&self.pool)
        .await?;
        let mut environment: BTreeMap<String, String> = stored
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default();
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
}

pub(crate) fn tui_config_args(config: &Value) -> Result<String> {
    let mut args = String::new();
    for (key, value) in config.as_object().expect("Session config is an object") {
        let value = toml::Value::try_from(value)
            .map_err(|error| Error::Domain(format!("Invalid Codex environment config: {error}")))?;
        let argument = format!("{key}={value}");
        args.push_str(" -c '");
        args.push_str(&argument.replace('\'', "'\\''"));
        args.push('\'');
    }
    Ok(args)
}
