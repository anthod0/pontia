use crate::runtime::protocol::Connection;
use pontia_application::interactions::{
    ChoiceInteractionRequest, InteractionIntent, InteractionOption, InteractionService,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub(crate) struct NativeInteraction {
    pub session: String,
    pub thread: String,
    pub turn: Option<String>,
    pub request_id: Value,
    method: String,
    permissions: Value,
}

struct Approval {
    thread: String,
    native_turn: Option<String>,
    title: String,
    description: Option<String>,
    permissions: Value,
}

fn approval(value: &Value) -> Option<Approval> {
    let method = value["method"].as_str()?;
    let params = &value["params"];
    let (thread, title, description) = match method {
        "item/commandExecution/requestApproval" => (
            params["threadId"].as_str()?,
            "Allow command execution?",
            params["command"]
                .as_str()
                .or(params["reason"].as_str())
                .map(str::to_owned),
        ),
        "item/fileChange/requestApproval" => (
            params["threadId"].as_str()?,
            "Allow file changes?",
            params["reason"].as_str().map(str::to_owned),
        ),
        "item/permissions/requestApproval" => (
            params["threadId"].as_str()?,
            "Allow additional permissions?",
            Some(serde_json::to_string(&params["permissions"]).ok()?),
        ),
        "execCommandApproval" => (
            params["conversationId"].as_str()?,
            "Allow command execution?",
            params["command"]
                .as_array()
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .or_else(|| params["reason"].as_str().map(str::to_owned)),
        ),
        "applyPatchApproval" => (
            params["conversationId"].as_str()?,
            "Allow file changes?",
            params["reason"].as_str().map(str::to_owned),
        ),
        _ => return None,
    };
    value
        .get("id")
        .filter(|id| id.is_string() || id.is_number())?;
    let mut permissions = json!({});
    for key in ["network", "fileSystem"] {
        if let Some(value) = params["permissions"]
            .get(key)
            .filter(|v| !v.is_null() && !v.as_object().is_some_and(|o| o.is_empty()))
        {
            permissions[key] = value.clone();
        }
    }
    Some(Approval {
        thread: thread.into(),
        native_turn: params["turnId"].as_str().map(str::to_owned),
        title: title.into(),
        description,
        permissions,
    })
}

pub(crate) async fn register(
    service: &InteractionService,
    pool: &sqlx::SqlitePool,
    connection: &Connection,
    value: &Value,
) -> Result<bool> {
    let Some(Approval {
        thread,
        native_turn,
        title,
        description,
        permissions,
    }) = approval(value)
    else {
        return Ok(false);
    };
    if !connection
        .pending_server_requests()
        .await
        .iter()
        .any(|request| request["id"] == value["id"])
    {
        return Ok(true);
    }
    let session: Option<String> = sqlx::query_scalar("SELECT a.session_id FROM agent_bindings a JOIN sessions s USING(session_id) WHERE a.client_type='codex' AND a.client_session_key=? AND s.state<>'exited'").bind(&thread).fetch_optional(pool).await?;
    let Some(session) = session else {
        return Ok(true);
    };
    let turn_id =
        match &native_turn {
            Some(turn) => sqlx::query_scalar::<_, String>(
                "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
            )
            .bind(&session)
            .bind(turn)
            .fetch_optional(pool)
            .await?,
            None => None,
        };
    let mut associations = connection.interactions.lock().await;
    if associations.values().any(|i| i.request_id == value["id"]) {
        return Ok(true);
    }
    let interaction = service
        .register(ChoiceInteractionRequest {
            session_id: session.clone(),
            turn_id,
            title,
            description,
            options: vec![
                InteractionOption {
                    key: "allow".into(),
                    label: "Allow".into(),
                    intent: InteractionIntent::Positive,
                },
                InteractionOption {
                    key: "deny".into(),
                    label: "Deny".into(),
                    intent: InteractionIntent::Negative,
                },
            ],
        })
        .await?;
    associations.insert(
        interaction.interaction_id,
        NativeInteraction {
            session,
            thread,
            turn: native_turn,
            request_id: value["id"].clone(),
            method: value["method"].as_str().unwrap().into(),
            permissions,
        },
    );
    Ok(true)
}

pub(crate) async fn reconcile(
    service: &InteractionService,
    pool: &sqlx::SqlitePool,
    connection: &Connection,
) -> Result<()> {
    let requests = connection.pending_server_requests().await;
    expire_matching(service, connection, |i| {
        !requests.iter().any(|r| r["id"] == i.request_id)
    })
    .await?;
    for request in requests {
        register(service, pool, connection, &request.value).await?;
    }
    Ok(())
}

pub(crate) async fn expire_matching(
    service: &InteractionService,
    connection: &Connection,
    matches: impl Fn(&NativeInteraction) -> bool,
) -> Result<()> {
    let mut associations = connection.interactions.lock().await;
    let ids: Vec<String> = associations
        .iter()
        .filter(|(_, i)| matches(i))
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Some(native) = associations.remove(&id) {
            service.expire(&native.session, &id).await?;
        }
    }
    Ok(())
}

pub(crate) async fn resolve(
    connection: &Connection,
    session: &str,
    id: &str,
    option: &str,
) -> Result<()> {
    let mut associations = connection.interactions.lock().await;
    let native = associations
        .get(id)
        .filter(|i| i.session == session)
        .ok_or_else(|| Error::CapabilityUnavailable("Native interaction expired".into()))?;
    let result = match (native.method.as_str(), option) {
        ("item/commandExecution/requestApproval" | "item/fileChange/requestApproval", "allow") => {
            json!({"decision":"accept"})
        }
        ("item/commandExecution/requestApproval" | "item/fileChange/requestApproval", "deny") => {
            json!({"decision":"decline"})
        }
        ("execCommandApproval" | "applyPatchApproval", "allow") => json!({"decision":"approved"}),
        ("execCommandApproval" | "applyPatchApproval", "deny") => json!({"decision":"denied"}),
        ("item/permissions/requestApproval", "allow") => {
            json!({"permissions":native.permissions,"scope":"turn"})
        }
        ("item/permissions/requestApproval", "deny") => json!({"permissions":{},"scope":"turn"}),
        _ => return Err(Error::Domain("Invalid interaction option".into())),
    };
    let request_id = native.request_id.clone();
    // Hold the association lock through the write acknowledgment: our own resolved
    // notification must not expire a choice whose response was successfully sent.
    let written = connection.reply(request_id, result).await;
    associations.remove(id);
    written
}
