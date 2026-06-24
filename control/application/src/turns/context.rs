use serde_json::{Value, json};
use sqlx::SqlitePool;

use pontia_agent_clients::get_client_spec;
use pontia_core::error::{Error, Result};
use pontia_runtime::AgentInput;
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

pub(crate) async fn store_client_current_turn_context(
    pool: SqlitePool,
    session_id: &str,
    metadata: &Value,
    input: &AgentInput,
    client_type: &str,
    turn_metadata: Option<&Value>,
) -> Result<()> {
    let mut metadata = metadata.clone();
    let context = client_current_turn_context(&metadata, input, client_type, turn_metadata)?;
    metadata["pending_current_turn"] = context;
    SqliteRuntimeBindingRepository::new(pool)
        .update_metadata(session_id, &serde_json::to_string(&metadata)?)
        .await?;
    Ok(())
}

fn client_current_turn_context(
    metadata: &Value,
    input: &AgentInput,
    client_type: &str,
    turn_metadata: Option<&Value>,
) -> Result<Value> {
    let internal_event_url = metadata["internal_event_url"]
        .as_str()
        .map(ToString::to_string)
        .or_else(pontia_runtime::configured_internal_event_url)
        .unwrap_or_else(|| "http://127.0.0.1:8080/internal/v1/events".to_string());
    let runtime_instance_id = metadata["runtime_instance_id"].as_str().ok_or_else(|| {
        Error::Domain(format!(
            "{client_type} runtime metadata missing runtime_instance_id"
        ))
    })?;
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
        context["turn_id"] = json!(input.turn_id);
    }
    if let Some(inbox_message_id) = turn_metadata
        .and_then(|metadata| metadata.get("inbox_message_id"))
        .and_then(Value::as_str)
    {
        context["inbox_message_id"] = json!(inbox_message_id);
    }
    Ok(context)
}
