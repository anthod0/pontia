use super::ThreadOperation;
use crate::{
    runtime::CodexRuntime,
    service::{CodexService, environment::environment_config, instructions},
};
use pontia_agent_control::{AgentBindingService, AgentControl};
use pontia_core::{Error, Result, SessionMode};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) struct Prepared {
    pub runtime: std::sync::Arc<CodexRuntime>,
    pub session: String,
    pub bound: bool,
    pub session_instructions: Option<Value>,
    _identity: Option<tokio::sync::OwnedMutexGuard<()>>,
}

pub(super) async fn prepare(
    state: &AgentControl,
    operation: ThreadOperation,
    params: &mut Value,
) -> Result<Option<Prepared>> {
    if params["ephemeral"] == true {
        if operation == ThreadOperation::Resume
            && params["path"].as_str().is_some_and(|path| !path.is_empty())
        {
            return Err(Error::StateConflict(
                "Ephemeral Codex resume cannot select a stored rollout".into(),
            ));
        }
        if operation == ThreadOperation::Resume
            && let Some(thread) = params["threadId"].as_str()
            && let Some(binding) = AgentBindingService::new(state.db())
                .binding_for_client_session("codex", thread)
                .await?
            && state.queries().session_mode(&binding.session_id).await? == SessionMode::Restricted
        {
            return Err(Error::StateConflict(
                "Cannot resume a restricted Codex session as ephemeral".into(),
            ));
        }
        return Ok(None);
    }
    let runtime =
        CodexRuntime::ensure(state.pontia_home(), crate::runtime::CodexServerTarget::User).await?;
    let identity = if operation == ThreadOperation::Resume {
        let native = params["threadId"]
            .as_str()
            .ok_or_else(|| Error::StateConflict("Codex resume has no thread ID".into()))?;
        Some(runtime.lock_session(&format!("proxy:{native}")).await)
    } else {
        None
    };
    let source = if operation != ThreadOperation::Start {
        AgentBindingService::new(state.db())
            .binding_for_client_session("codex", params["threadId"].as_str().unwrap_or_default())
            .await?
    } else {
        None
    };
    let mode = match &source {
        Some(binding) if operation == ThreadOperation::Resume => {
            state.queries().session_mode(&binding.session_id).await?
        }
        _ => SessionMode::Default,
    };
    if operation == ThreadOperation::Resume
        && let Some(path) = params["path"].as_str().filter(|path| !path.is_empty())
    {
        // On cold resume Codex prefers the rollout path to threadId. Forward by
        // verified ID so the native thread and the Session policy always match.
        let snapshot = runtime
            .connection()
            .await?
            .call(
                "thread/read",
                json!({"threadId":params["threadId"],"includeTurns":false}),
            )
            .await?;
        if snapshot["thread"]["path"].as_str() != Some(path) {
            return Err(Error::StateConflict(
                "Codex resume path and thread ID differ".into(),
            ));
        }
        params.as_object_mut().unwrap().remove("path");
    }
    if let Some(binding) = &source
        && crate::runtime::CodexServerTarget::for_session(&state.db(), &binding.session_id).await?
            != runtime.target
    {
        if mode == SessionMode::Restricted {
            return Err(Error::StateConflict(
                "Restricted Codex session belongs to another server".into(),
            ));
        }
        return Ok(None);
    }
    let binding = source.filter(|_| operation == ThreadOperation::Resume);
    let session = binding
        .as_ref()
        .map(|binding| binding.session_id.clone())
        .unwrap_or_else(|| pontia_core::ids::new_session_id().to_string());
    let connection = runtime.connection().await?;
    if let Some(binding) = &binding
        && mode == SessionMode::Restricted
    {
        pontia_agent_control::workspaces::require_active_workspace(
            &state.db(),
            &binding.launch_cwd,
        )
        .await?;
        let config = CodexService::new(state.event_ingest_service())
            .session_config(&session, &connection, &binding.launch_cwd)
            .await?;
        // Rebuild launch options from the bound Session. CLI config and profiles can
        // introduce tools outside the configuration inspected by session_config.
        params.as_object_mut().unwrap().retain(|key, _| {
            matches!(
                key.as_str(),
                "threadId"
                    | "excludeTurns"
                    | "initialTurnsPage"
                    | "model"
                    | "serviceTier"
                    | "personality"
            )
        });
        params["cwd"] = json!(binding.launch_cwd);
        params["approvalPolicy"] = json!("never");
        params["sandbox"] = json!("danger-full-access");
        params["config"] = config;
        return Ok(Some(Prepared {
            runtime,
            session,
            bound: true,
            session_instructions: None,
            _identity: identity,
        }));
    }
    let native_cwd = if params["cwd"].as_str().is_none() && operation != ThreadOperation::Start {
        if let Some(native) = params["threadId"].as_str() {
            match connection
                .call(
                    "thread/read",
                    json!({"threadId":native,"includeTurns":false}),
                )
                .await
            {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    tracing::debug!(%error, "Codex Workspace preflight unavailable; forwarding native request");
                    return Ok(None);
                }
            }
        } else {
            None
        }
    } else {
        None
    };
    let cwd = params["cwd"].as_str().or_else(|| {
        native_cwd
            .as_ref()
            .and_then(|value| value["thread"]["cwd"].as_str())
    });
    let Some(cwd) = cwd else { return Ok(None) };
    match pontia_agent_control::workspaces::require_active_workspace(&state.db(), cwd).await {
        Ok(_) => {}
        Err(Error::NotFound(_) | Error::StateConflict(_) | Error::Io(_)) => return Ok(None),
        Err(error) => return Err(error),
    }
    let response = connection
        .call("config/read", json!({"cwd":cwd,"includeLayers":false}))
        .await?;
    if mode.bypasses_approvals() {
        params["approvalPolicy"] = json!("never");
        params["sandbox"] = json!("danger-full-access");
    }
    let session_instructions =
        instructions::prepare_proxy_request(state, operation, params, &response["config"]).await?;
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
    let mut environment: BTreeMap<String, String> = effective["shell_environment_policy"]["set"]
        .as_object()
        .map(|value| serde_json::from_value(json!(value)))
        .transpose()?
        .unwrap_or_default();
    if binding.is_some() {
        environment.extend(state.queries().runtime_environment(&session).await?);
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
        runtime,
        bound: binding.is_some(),
        session,
        session_instructions,
        _identity: identity,
    }))
}

pub(crate) struct Reservation {
    pub close: tokio::sync::watch::Sender<bool>,
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
