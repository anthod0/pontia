use serde_json::{Value, json};
use sqlx::SqlitePool;

use pontia_agent_clients::get_client_spec;
use pontia_core::error::{Error, Result};
use pontia_runtime::AgentInput;
use pontia_storage_sqlite::repositories::runtime_bindings::{
    PendingTurnContextRecord, SqliteRuntimeBindingRepository,
};

pub(crate) async fn store_client_current_turn_context(
    pool: SqlitePool,
    session_id: &str,
    input: &AgentInput,
    client_type: &str,
    turn_metadata: Option<&Value>,
) -> Result<()> {
    let repository = SqliteRuntimeBindingRepository::new(pool);
    let runtime = repository
        .runtime_context(session_id)
        .await?
        .ok_or_else(|| {
            Error::NotFound(format!(
                "runtime binding for session {session_id} not found"
            ))
        })?;
    let runtime_instance_id = runtime.runtime_instance_id.ok_or_else(|| {
        Error::Domain(format!(
            "{client_type} runtime binding missing runtime_instance_id"
        ))
    })?;
    let internal_event_url = runtime
        .internal_event_url
        .filter(|value| !value.trim().is_empty())
        .or_else(pontia_runtime::configured_internal_event_url)
        .unwrap_or_else(|| "http://127.0.0.1:8080/internal/v1/events".to_string());
    let mut context = json!({
        "session_id": input.session_id,
        "input": input.input,
        "client_type": client_type,
        "runtime_instance_id": runtime_instance_id,
        "internal_event_url": internal_event_url,
    });
    let include_turn_id = get_client_spec(client_type)
        .map(|spec| spec.current_turn_context_includes_turn_id())
        .unwrap_or(true);
    if include_turn_id {
        context["turn_id"] = json!(input.dispatch_id);
    }
    if let Some(inbox_message_id) = turn_metadata
        .and_then(|metadata| metadata.get("inbox_message_id"))
        .and_then(Value::as_str)
    {
        context["inbox_message_id"] = json!(inbox_message_id);
    }

    repository
        .store_pending_turn_context(PendingTurnContextRecord {
            session_id: session_id.to_string(),
            runtime_instance_id,
            client_type: client_type.to_string(),
            payload: serde_json::to_string(&context)?,
        })
        .await
}
