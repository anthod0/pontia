use serde_json::Value;

use pontia_core::{
    domain::{DomainEvent, EventSource, EventType},
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository, runtime_bindings::SqliteRuntimeBindingRepository,
    sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};

pub(super) async fn validate_turn_identity_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &DomainEvent,
    require_existing_followup: bool,
) -> Result<()> {
    if !event.event_type.is_turn_event() {
        return Ok(());
    }
    let turn_id = event.turn_id.as_deref().expect("validated turn_id");

    match SqliteTurnRepository::turn_session_id_in_tx(tx, turn_id).await? {
        Some(session_id) if session_id != event.session_id => Err(Error::Domain(format!(
            "turn {turn_id} belongs to session {session_id}, not {}",
            event.session_id
        ))),
        Some(_) => Ok(()),
        None if event_type_can_create_turn(event.event_type) || !require_existing_followup => {
            Ok(())
        }
        None => Err(Error::Domain(format!(
            "{} references unknown turn {turn_id} in session {}",
            event.event_type, event.session_id
        ))),
    }
}

pub(super) async fn ensure_runtime_fence_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &DomainEvent,
) -> Result<()> {
    if !is_confirmed_runtime_source(event.source)
        || !runtime_instance_id_required_for_event(event.event_type)
    {
        return Ok(());
    }
    let expected_runtime_instance_id =
        SqliteRuntimeBindingRepository::runtime_instance_id_in_tx(tx, &event.session_id).await?;
    let Some(expected_runtime_instance_id) = expected_runtime_instance_id else {
        if event.event_type == EventType::SessionReady {
            return Err(Error::Domain(format!(
                "{} from {} requires a confirmed Runtime binding for session {}",
                event.event_type, event.source, event.session_id
            )));
        }
        return Ok(());
    };
    let provided_runtime_instance_id = event
        .payload
        .get("runtime_instance_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::Domain(format!(
                "{} from {} requires payload.runtime_instance_id for runtime-bound session {}",
                event.event_type, event.source, event.session_id
            ))
        })?;
    if provided_runtime_instance_id != expected_runtime_instance_id {
        return Err(Error::Domain(format!(
            "payload.runtime_instance_id does not match session {} runtime binding",
            event.session_id
        )));
    }
    Ok(())
}

pub(super) async fn ensure_confirmed_event_matches_session_boundary(
    pool: &sqlx::SqlitePool,
    event: &DomainEvent,
) -> Result<()> {
    if !is_confirmed_runtime_source(event.source) || event.event_type == EventType::SessionCreated {
        return Ok(());
    }

    let session = SqliteSessionRepository::new(pool.clone())
        .get_session(&event.session_id)
        .await?;
    let Some(session) = session else {
        return Err(Error::Domain(format!(
            "{} from {} references unknown session {}",
            event.event_type, event.source, event.session_id
        )));
    };
    if event.client_type != session.client_type {
        return Err(Error::Domain(format!(
            "{} from {} has client_type {} but session {} uses client_type {}",
            event.event_type,
            event.source,
            event.client_type,
            event.session_id,
            session.client_type
        )));
    }

    if event.event_type == EventType::SessionReady {
        ensure_ready_identity_matches_bindings(pool, event).await?;
    }

    let expected_runtime_instance_id = SqliteRuntimeBindingRepository::new(pool.clone())
        .runtime_instance_id(&event.session_id)
        .await?;

    let Some(expected_runtime_instance_id) = expected_runtime_instance_id else {
        if event.event_type == EventType::SessionReady {
            return Err(Error::Domain(format!(
                "{} from {} requires a confirmed Runtime binding for session {}",
                event.event_type, event.source, event.session_id
            )));
        }
        return Ok(());
    };

    let provided_runtime_instance_id = event
        .payload
        .get("runtime_instance_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if runtime_instance_id_required_for_event(event.event_type) {
        let Some(provided_runtime_instance_id) = provided_runtime_instance_id else {
            return Err(Error::Domain(format!(
                "{} from {} requires payload.runtime_instance_id for runtime-bound session {}",
                event.event_type, event.source, event.session_id
            )));
        };
        if provided_runtime_instance_id != expected_runtime_instance_id {
            return Err(Error::Domain(format!(
                "payload.runtime_instance_id does not match session {} runtime binding",
                event.session_id
            )));
        }
    }

    Ok(())
}

async fn ensure_ready_identity_matches_bindings(
    pool: &sqlx::SqlitePool,
    event: &DomainEvent,
) -> Result<()> {
    let Some(client_session_key) = event
        .payload
        .get("client_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let agent_bindings = SqliteAgentBindingRepository::new(pool.clone());
    if let Some(binding) = agent_bindings
        .binding_for_session(&event.session_id)
        .await?
        && (binding.client_type != event.client_type
            || binding.client_session_key != client_session_key)
    {
        return Err(Error::Domain(format!(
            "session.ready client identity does not match session {} Agent binding",
            event.session_id
        )));
    }
    if let Some(binding) = agent_bindings
        .binding_for_client_session(&event.client_type, client_session_key)
        .await?
        && binding.session_id != event.session_id
    {
        return Err(Error::Domain(format!(
            "session.ready client identity is already bound to another Session {}",
            binding.session_id
        )));
    }

    if let Some(runtime_metadata) = SqliteRuntimeBindingRepository::new(pool.clone())
        .metadata(&event.session_id)
        .await?
        && crate::agent_bindings::runtime_binding_identity_disagrees(
            &runtime_metadata,
            client_session_key,
        )?
    {
        return Err(Error::Domain(format!(
            "session.ready client identity does not match session {} Runtime binding",
            event.session_id
        )));
    }

    Ok(())
}

fn is_confirmed_runtime_source(source: EventSource) -> bool {
    matches!(
        source,
        EventSource::AgentAdapter | EventSource::AgentClient | EventSource::RuntimeManager
    )
}

fn runtime_instance_id_required_for_event(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::SessionReady | EventType::SessionExited | EventType::TurnStarted
    )
}

fn event_type_can_create_turn(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::TurnCreated | EventType::TurnQueued | EventType::TurnStarted
    )
}
