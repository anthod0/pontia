use super::*;
use crate::agent_clients::{DispatchMode, get_client_spec};

#[derive(Clone)]
pub struct TurnCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl TurnCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub(crate) async fn create_and_dispatch_turn(
        &self,
        session_id: &str,
        input: String,
        metadata: Value,
    ) -> Result<TurnView> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "idle" | "interrupted") {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot accept a new turn",
                session.state
            )));
        }

        if let Some(active_turn_id) = &session.current_turn_id {
            return Err(Error::StateConflict(format!(
                "session {session_id} already has active turn {active_turn_id}"
            )));
        }

        if !session.capabilities.accept_task {
            return Err(Error::Domain(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }

        let turn_id = new_turn_id().to_string();
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.clone(),
            input: input.clone(),
        };
        let dispatch_mode = get_client_spec(&session.client_type)
            .map(|spec| spec.dispatch_mode)
            .ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", session.client_type))
            })?;
        if dispatch_mode == DispatchMode::GenericTestAdapter {
            self.runtime.submit_input(agent_input.clone())?;
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnCreated,
                json!({
                    "input": { "summary": input },
                    "metadata": metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnQueued,
                json!({}),
            ))
            .await?;

        if dispatch_mode == DispatchMode::TmuxPaste {
            match self.runtime_binding_metadata(session_id).await? {
                Some((runtime_ref, binding_metadata)) => {
                    match write_client_current_turn_context(
                        &binding_metadata,
                        &agent_input,
                        &session.client_type,
                    )
                    .and_then(|()| {
                        self.runtime.dispatch_tui_turn(
                            &runtime_ref,
                            &session.client_type,
                            &agent_input,
                        )
                    }) {
                        Ok(()) => {
                            ingest
                                .ingest_event(DomainEvent::new(
                                    new_event_id().to_string(),
                                    session_id.to_string(),
                                    Some(turn_id.clone()),
                                    EventSource::AgentAdapter,
                                    session.client_type.clone(),
                                    EventType::TurnStarted,
                                    json!({}),
                                ))
                                .await?;
                        }
                        Err(error) => {
                            ingest
                                .ingest_event(DomainEvent::new(
                                    new_event_id().to_string(),
                                    session_id.to_string(),
                                    Some(turn_id.clone()),
                                    EventSource::RuntimeManager,
                                    session.client_type.clone(),
                                    EventType::TurnFailed,
                                    json!({ "failure": { "message": error.to_string() } }),
                                ))
                                .await?;
                        }
                    }
                }
                None => {
                    ingest
                        .ingest_event(DomainEvent::new(
                            new_event_id().to_string(),
                            session_id.to_string(),
                            Some(turn_id.clone()),
                            EventSource::RuntimeManager,
                            session.client_type.clone(),
                            EventType::TurnFailed,
                            json!({ "failure": { "message": format!("{} runtime binding not found", session.client_type) } }),
                        ))
                        .await?;
                }
            }
        }

        let mut turn = query
            .get_turn(session_id, &turn_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted turn missing".to_string()))?;
        query.enrich_turn_view(&mut turn).await?;
        Ok(turn)
    }

    async fn runtime_binding_metadata(&self, session_id: &str) -> Result<Option<(String, Value)>> {
        let row =
            sqlx::query("SELECT runtime_ref, metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|row| {
            let runtime_ref: String = row.try_get("runtime_ref")?;
            let metadata: String = row.try_get("metadata")?;
            let metadata = serde_json::from_str(&metadata)?;
            Ok((runtime_ref, metadata))
        })
        .transpose()
    }
}

pub(crate) fn write_client_current_turn_context(
    metadata: &Value,
    input: &AgentInput,
    client_type: &str,
) -> Result<()> {
    let current_turn_file = metadata["current_turn_file"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| {
            metadata["runtime_dir"]
                .as_str()
                .map(|runtime_dir| Path::new(runtime_dir).join("current-turn.json"))
        })
        .ok_or_else(|| {
            Error::Domain(format!(
                "{client_type} runtime metadata missing current_turn_file"
            ))
        })?;
    if let Some(parent) = current_turn_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let internal_event_url = metadata["internal_event_url"]
        .as_str()
        .unwrap_or("http://127.0.0.1:8080/internal/v1/events");
    let runtime_instance_id = metadata["runtime_instance_id"].as_str().ok_or_else(|| {
        Error::Domain(format!(
            "{client_type} runtime metadata missing runtime_instance_id"
        ))
    })?;
    let context = json!({
        "session_id": input.session_id,
        "turn_id": input.turn_id,
        "input": input.input,
        "client_type": client_type,
        "runtime_instance_id": runtime_instance_id,
        "internal_event_url": internal_event_url,
    });
    std::fs::write(current_turn_file, serde_json::to_vec_pretty(&context)?)?;
    Ok(())
}
