use super::CodexService;
use crate::runtime::protocol::Connection;
use pontia_core::{Error, Result, SessionMode};
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
        let mode = pontia_agent_control::ControlQueryService::new(self.pool.clone())
            .session_mode(session)
            .await?;
        let response = match connection
            .call(
                "config/read",
                json!({"cwd":cwd,"includeLayers":mode == SessionMode::Restricted}),
            )
            .await
        {
            Err(error) if mode == SessionMode::Restricted => {
                return Err(Error::StateConflict(format!(
                    "Cannot enumerate Codex MCP servers for the restricted session: {error}"
                )));
            }
            result => result?,
        };
        let environment = pontia_agent_control::ControlQueryService::new(self.pool.clone())
            .runtime_environment(session)
            .await?;
        let mut config = environment_config(session, &response["config"], environment)?;
        if mode == SessionMode::Restricted {
            disable_mcp_servers(&mut config, &response)?;
            config["skills.config"] = disabled_skills(connection, cwd, &response["config"]).await?;
            for key in [
                "agents.enabled",
                "features.multi_agent",
                "features.multi_agent_v2",
                "features.apps",
                "features.plugins",
                "features.skill_search",
                "features.skill_mcp_dependency_install",
                "skills.bundled.enabled",
                "skills.include_instructions",
                "features.goals",
                "features.image_generation",
                "features.view_image",
                "features.sleep_tool",
                "features.tool_suggest",
                "tools.update_plan.enabled",
                "tools.experimental_request_user_input.enabled",
            ] {
                config[key] = json!(false);
            }
            config["features.skip_host_skill_discovery"] = json!(true);
            config["web_search"] = json!("disabled");
        }
        if mode.bypasses_approvals() {
            config["approval_policy"] = json!("never");
            config["sandbox_mode"] = json!("danger-full-access");
        }
        if let Some(instructions) = self.instructions().codex_binding(session).await?
            && let Some(prompt) = instructions.system_prompt
        {
            let existing = instructions
                .native_developer_instructions
                .as_deref()
                .or_else(|| response["config"]["developer_instructions"].as_str());
            config["developer_instructions"] =
                json!(super::instructions::append_instructions(existing, &prompt));
        }
        Ok(config)
    }
}

fn disable_mcp_servers(config: &mut Value, response: &Value) -> Result<()> {
    let servers = response["config"]["mcp_servers"]
        .as_object()
        .ok_or_else(|| {
            Error::StateConflict(
                "Codex config/read expected an object at config.mcp_servers".into(),
            )
        })?;
    for name in servers.keys() {
        config["mcp_servers"][name]["enabled"] = json!(false);
    }
    // Starting a full-access thread can activate a previously untrusted project layer.
    // Codex validates the transport even for a disabled server missing from effective config.
    for layer in response["layers"].as_array().into_iter().flatten() {
        if layer["name"]["type"] != "project" || layer["disabledReason"].is_null() {
            continue;
        }
        if let Some(project_servers) = layer["config"]["mcp_servers"].as_object() {
            for (name, server) in project_servers {
                if config["mcp_servers"].get(name).is_none()
                    && let Some(key) = ["command", "url"]
                        .into_iter()
                        .find(|key| server[*key].is_string())
                {
                    config["mcp_servers"][name] = json!({(key):server[key], "enabled":false});
                }
            }
        }
    }
    Ok(())
}

async fn disabled_skills(
    connection: &Connection,
    cwd: &str,
    configuration: &Value,
) -> Result<Value> {
    #[derive(Deserialize)]
    struct Skill {
        name: String,
    }

    let listed = connection
        .call("skills/list", json!({"cwds":[cwd],"forceReload":true}))
        .await?;
    let skills: Vec<Skill> = serde_json::from_value(listed["data"][0]["skills"].clone())?;
    // The thread override replaces the native rule list. Keep existing disables,
    // including rules for skills outside the current catalog.
    let mut rules: Vec<_> = configuration["skills"]["config"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|rule| rule["enabled"] == false)
        .cloned()
        .collect();
    rules.extend(
        skills
            .into_iter()
            .map(|skill| json!({"name":skill.name,"enabled":false})),
    );
    Ok(json!(rules))
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
