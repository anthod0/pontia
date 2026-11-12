use super::ThreadOperation;
use crate::{
    runtime::CodexRuntime,
    service::{environment::environment_config, profile},
};
use pontia_agent_control::{AgentBindingService, AgentControl};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) struct Prepared {
    pub session: String,
    pub bound: bool,
    pub profile_binding: Option<Value>,
    _identity: Option<tokio::sync::OwnedMutexGuard<()>>,
}

pub(super) async fn prepare(
    state: &AgentControl,
    operation: ThreadOperation,
    params: &mut Value,
) -> Result<Option<Prepared>> {
    if params["ephemeral"] == true {
        return Ok(None);
    }
    let runtime = CodexRuntime::ensure(state.pontia_home()).await?;
    let identity = if operation == ThreadOperation::Resume {
        let native = params["threadId"]
            .as_str()
            .ok_or_else(|| Error::StateConflict("Codex resume has no thread ID".into()))?;
        Some(runtime.lock_session(&format!("proxy:{native}")).await)
    } else {
        None
    };
    let binding = if operation == ThreadOperation::Resume {
        AgentBindingService::new(state.db())
            .binding_for_client_session("codex", params["threadId"].as_str().unwrap())
            .await?
    } else {
        None
    };
    let session = binding
        .as_ref()
        .map(|binding| binding.session_id.clone())
        .unwrap_or_else(|| pontia_core::ids::new_session_id().to_string());
    let connection = runtime.connection().await?;
    let cwd = params["cwd"]
        .as_str()
        .or_else(|| binding.as_ref().map(|binding| binding.launch_cwd.as_str()));
    let response = connection
        .call("config/read", json!({"cwd":cwd,"includeLayers":false}))
        .await?;
    let profile_binding =
        profile::prepare_proxy_request(state, operation, params, &response["config"]).await?;
    let mut effective = response["config"].clone();
    if params["config"].is_null() {
        params["config"] = json!({});
    }
    let overrides = params["config"]
        .as_object_mut()
        .ok_or_else(|| Error::StateConflict("Codex config must be an object".into()))?;
    let mut requested = json!({});
    for (key, value) in overrides.iter() {
        if key == "shell_environment_policy" || key.starts_with("shell_environment_policy.") {
            set_override(&mut effective, key, value.clone());
            set_override(&mut requested, key, value.clone());
        }
    }
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT json_extract(metadata,'$.codex_environment') FROM sessions WHERE session_id=?",
    )
    .bind(&session)
    .fetch_optional(&state.db())
    .await?
    .flatten();
    let mut environment: BTreeMap<String, String> = effective["shell_environment_policy"]["set"]
        .as_object()
        .map(|value| serde_json::from_value(json!(value)))
        .transpose()?
        .unwrap_or_default();
    if let Some(stored) = stored {
        environment.extend(serde_json::from_str::<BTreeMap<String, String>>(&stored)?);
    }
    for (key, value) in environment_config(&session, &effective, environment)?
        .as_object()
        .unwrap()
    {
        set_override(&mut requested, key, value.clone());
    }
    overrides.retain(|key, _| {
        key != "shell_environment_policy" && !key.starts_with("shell_environment_policy.")
    });
    overrides.insert(
        "shell_environment_policy".into(),
        requested["shell_environment_policy"].clone(),
    );
    Ok(Some(Prepared {
        bound: binding.is_some(),
        session,
        profile_binding,
        _identity: identity,
    }))
}

pub(crate) struct Reservation {
    pub session: String,
    pub bound: tokio::sync::oneshot::Sender<()>,
}

fn set_override(target: &mut Value, path: &str, value: Value) {
    if let Some((key, rest)) = path.split_once('.') {
        set_override(&mut target[key], rest, value);
    } else if let (Some(target), Some(value)) = (target[path].as_object_mut(), value.as_object()) {
        for (key, value) in value {
            let entry = target.entry(key.clone()).or_insert(Value::Null);
            if entry.is_object() && value.is_object() {
                for (key, value) in value.as_object().unwrap() {
                    set_override(entry, key, value.clone());
                }
            } else {
                *entry = value.clone();
            }
        }
    } else {
        target[path] = value;
    }
}
