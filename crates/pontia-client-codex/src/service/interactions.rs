use crate::runtime::protocol::Connection;
use pontia_application::interactions::{
    ChoiceInteractionRequest, InteractionIntent, InteractionOption, InteractionService,
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

pub(crate) struct NativeInteraction {
    pub session: String,
    pub thread: String,
    pub turn: Option<String>,
    pub request_id: Value,
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

async fn register(
    service: &InteractionService,
    ingest: &pontia_application::EventIngestService,
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
    let turn_id =
        match &native_turn {
            Some(turn) => sqlx::query_scalar::<_, String>(
                "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
            )
            .bind(&session)
            .bind(turn)
            .fetch_optional(&pool)
            .await?,
            None => None,
        };
    let mut associations = connection.interactions.lock().await;
    if associations.values().any(|i| i.request_id == value["id"]) {
        return Ok(());
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
    if let Err(error) = ingest
        .report_client_fact(
            &session,
            None,
            EventType::InteractionBlocked,
            json!({"interaction_id": interaction.interaction_id}),
        )
        .await
    {
        service
            .expire(&session, &interaction.interaction_id)
            .await?;
        return Err(error);
    }
    associations.insert(
        interaction.interaction_id,
        NativeInteraction {
            session,
            thread,
            turn: native_turn,
            request_id: value["id"].clone(),
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
    ingest: &pontia_application::EventIngestService,
    connection: &Connection,
) -> Result<()> {
    loop {
        let Some(event) = connection.interaction_events.lock().await.front().cloned() else {
            break;
        };
        if event["method"] != "serverRequest/resolved" {
            register(service, ingest, connection, &event).await?;
            connection.interaction_events.lock().await.pop_front();
            continue;
        }
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
                .report_client_fact(
                    &native.session,
                    None,
                    EventType::InteractionUnblocked,
                    json!({"interaction_id": id}),
                )
                .await?;
            if native.deliverable {
                service.expire(&native.session, &id).await?;
            }
            associations.remove(&id);
        }
        connection.interaction_events.lock().await.pop_front();
    }
    Ok(())
}

pub(crate) async fn reconcile(
    service: &InteractionService,
    ingest: &pontia_application::EventIngestService,
    connection: &Connection,
) -> Result<()> {
    // Discover requests received before their Session binding became available.
    for request in connection.pending_server_requests().await {
        register(service, ingest, connection, &request.value).await?;
    }
    observe_native(service, ingest, connection).await?;
    let requests = connection.pending_server_requests().await;
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
    associations.get_mut(id).unwrap().deliverable = false;
    written
}
