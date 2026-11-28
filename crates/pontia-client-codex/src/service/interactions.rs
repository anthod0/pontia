use crate::runtime::protocol::Connection;
use pontia_agent_control::interactions::{
    ChoiceInteractionRequest, InteractionBlock, InteractionIntent, InteractionOption,
    InteractionService, InteractionState, authorization_digest, transient_storage_error,
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

pub(crate) struct NativeInteraction {
    pub session: String,
    pub thread: String,
    pub request_id: Value,
    digest: String,
    method: String,
    permissions: Value,
    deliverable: bool,
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

// JSON-RPC ids scope the key. The method and complete native params (including
// grantRoot and permission scopes) describe the authorization.
pub(crate) fn approval_digest(frame: &Value) -> String {
    authorization_digest(json!([frame["method"], frame["params"]]))
}

async fn register(
    service: &InteractionService,
    ingest: &pontia_agent_control::EventIngestService,
    connection: &Connection,
    value: &Value,
) -> Result<()> {
    let Some(Approval {
        thread,
        native_turn,
        title,
        description,
        permissions,
    }) = approval(value)
    else {
        return Ok(());
    };
    let pool = ingest.db();
    let session: Option<String> = sqlx::query_scalar("SELECT a.session_id FROM agent_bindings a JOIN sessions s USING(session_id) WHERE a.client_type='codex' AND a.client_session_key=? AND s.state<>'exited'").bind(&thread).fetch_optional(&pool).await?;
    let Some(session) = session else {
        return Ok(());
    };
    if crate::runtime::CodexServerTarget::for_session(&pool, &session).await? != connection.target
        || super::CodexService::has_confirmed_exit(&session).await
        || connection
            .unloaded_threads
            .lock()
            .await
            .contains_key(&thread)
    {
        return Ok(());
    }
    let key = format!("codex:{}:{}", connection.identity.instance_id, value["id"]);
    let digest = approval_digest(value);
    let existing = service.registration(&session, &key).await?;
    let turn_id = if let Some((interaction, _)) = &existing {
        interaction.turn_id.clone()
    } else {
        match &native_turn {
            Some(turn) => sqlx::query_scalar::<_, String>(
                "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
            )
            .bind(&session)
            .bind(turn)
            .fetch_optional(&pool)
            .await?,
            None => None,
        }
    };
    let registration = ChoiceInteractionRequest {
        report_key: key,
        authorization_digest: digest.clone(),
        blocked: Some(InteractionBlock {
            key_prefix: "codex".into(),
            runtime_id: None,
            turn_id: None,
        }),
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
    };
    let mut associations = connection.interactions.lock().await;
    let interaction = match service.register(registration).await {
        Ok(interaction) => interaction,
        Err(error) if transient_storage_error(&error) => return Err(error),
        Err(error) => {
            tracing::warn!(%session, %error, "Codex permission registration rejected");
            if let Some((original, _)) = &existing {
                if !matches!(
                    original.state,
                    InteractionState::Pending | InteractionState::Resolving
                ) {
                    connection.discard_replay(&value["id"], &digest).await;
                    return Ok(());
                }
                service.expire(&session, &original.interaction_id).await?;
                if let Some(native) = associations.get_mut(&original.interaction_id) {
                    native.deliverable = false;
                }
            }
            return Err(error);
        }
    };
    if matches!(
        interaction.state,
        InteractionState::Resolved | InteractionState::Expired
    ) {
        connection.discard_replay(&value["id"], &digest).await;
        return Ok(());
    }
    if associations.contains_key(&interaction.interaction_id) {
        return Ok(());
    }
    associations.insert(
        interaction.interaction_id,
        NativeInteraction {
            session,
            thread,
            request_id: value["id"].clone(),
            digest,
            method: value["method"].as_str().unwrap().into(),
            permissions,
            deliverable: true,
        },
    );
    Ok(())
}

// Only a matching native resolved notification proves this approval wait ended.
// Delivery expiry (including Turn end/unload) must retain this correlation.
pub(crate) async fn observe_native(
    service: &InteractionService,
    ingest: &pontia_agent_control::EventIngestService,
    connection: &Connection,
) -> Result<()> {
    let batch = std::mem::take(&mut *connection.interaction_events.lock().await);
    let mut deferred_ids = std::collections::HashSet::new();
    let mut retry = std::collections::VecDeque::new();
    for (event, was_deferred) in batch {
        let resolution = event["method"] == "serverRequest/resolved";
        let id = if resolution {
            &event["params"]["requestId"]
        } else {
            &event["id"]
        }
        .to_string();
        if was_deferred && !resolution && !connection.request_pending(&id).await {
            continue;
        }
        if deferred_ids.contains(&id) {
            retry.push_back((event, true));
            continue;
        }
        let result = if resolution {
            resolved(service, ingest, connection, &event).await
        } else {
            register(service, ingest, connection, &event).await
        };
        if let Err(error) = result {
            tracing::warn!(%error, request_id=%id, "Codex permission observation failed");
            if transient_storage_error(&error) {
                deferred_ids.insert(id);
                retry.push_back((event, true));
            } else if !resolution
                && let Ok(reply) = denial(&event)
                && let Err(error) = connection
                    .reply(event["id"].clone(), approval_digest(&event), reply)
                    .await
            {
                tracing::warn!(%error, request_id=%id, "Codex permission denial could not be confirmed");
            }
        }
    }
    if !retry.is_empty() {
        let mut events = connection.interaction_events.lock().await;
        retry.append(&mut *events);
        *events = retry;
    }
    Ok(())
}

async fn resolved(
    service: &InteractionService,
    ingest: &pontia_agent_control::EventIngestService,
    connection: &Connection,
    event: &Value,
) -> Result<()> {
    let mut associations = connection.interactions.lock().await;
    let ids: Vec<String> = associations
        .iter()
        .filter(|(_, native)| {
            event["params"]["threadId"] == native.thread
                && event["params"]["requestId"] == native.request_id
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        let native = &associations[&id];
        ingest
            .report_fact(pontia_agent_control::ReportedFact {
                session_id: native.session.clone(),
                turn_id: None,
                fact_type: EventType::InteractionUnblocked,
                report_key: Some(format!("codex:{id}:interaction.unblocked")),
                data: json!({"interaction_id": id}),
            })
            .await
            .map_err(|e| match e {
                pontia_agent_control::EventReportError::InvalidFact(message) => {
                    Error::Domain(message)
                }
                pontia_agent_control::EventReportError::Ingestion(error) => error,
            })?;
        if native.deliverable {
            service.expire(&native.session, &id).await?;
        }
        associations.remove(&id);
    }
    Ok(())
}

pub(crate) async fn reconcile(
    service: &InteractionService,
    ingest: &pontia_agent_control::EventIngestService,
    connection: &Connection,
) -> Result<()> {
    // Discover requests received before their Session binding became available.
    for request in connection
        .pending_server_requests()
        .await
        .unwrap_or_default()
    {
        if connection
            .interactions
            .lock()
            .await
            .values()
            .any(|i| i.request_id == request["id"])
        {
            continue;
        }
        let mut events = connection.interaction_events.lock().await;
        if !events.iter().any(|(event, _)| event["id"] == request["id"]) {
            events.push_back((request.value, false));
        }
    }
    observe_native(service, ingest, connection).await?;
    let Some(requests) = connection.pending_server_requests().await else {
        return Ok(());
    };
    expire_matching(service, connection, |i| {
        !requests.iter().any(|r| r["id"] == i.request_id)
    })
    .await?;
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
        .filter(|(_, i)| i.deliverable && matches(i))
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Some(native) = associations.get_mut(&id) {
            service.expire(&native.session, &id).await?;
            native.deliverable = false;
            tracing::debug!(session = %native.session, interaction_id = %id,
                "Codex approval delivery expired; retaining wait correlation until native resolution");
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
        .filter(|i| i.session == session && i.deliverable)
        .ok_or_else(|| Error::StateConflict("Native interaction expired".into()))?;
    let result = approval_response(&native.method, option, &native.permissions)?;
    let request_id = native.request_id.clone();
    // Hold the association lock through the write acknowledgment: our own resolved
    // notification must not expire a choice whose response was successfully sent.
    let written = connection
        .reply(request_id, native.digest.clone(), result)
        .await;
    associations.get_mut(id).unwrap().deliverable = false;
    written
}

pub(crate) fn denial(frame: &Value) -> Result<Value> {
    approval_response(frame["method"].as_str().unwrap_or(""), "deny", &Value::Null)
}

fn approval_response(method: &str, option: &str, permissions: &Value) -> Result<Value> {
    Ok(match (method, option) {
        ("item/commandExecution/requestApproval" | "item/fileChange/requestApproval", "allow") => {
            json!({"decision":"accept"})
        }
        ("item/commandExecution/requestApproval" | "item/fileChange/requestApproval", "deny") => {
            json!({"decision":"decline"})
        }
        ("execCommandApproval" | "applyPatchApproval", "allow") => json!({"decision":"approved"}),
        ("execCommandApproval" | "applyPatchApproval", "deny") => json!({"decision":"denied"}),
        ("item/permissions/requestApproval", "allow") => {
            json!({"permissions":permissions,"scope":"turn"})
        }
        ("item/permissions/requestApproval", "deny") => json!({"permissions":{},"scope":"turn"}),
        _ => return Err(Error::Domain("Invalid interaction option".into())),
    })
}
