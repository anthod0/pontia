use crate::EventIngestService;
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct NativeTurnService {
    events: EventIngestService,
    pool: SqlitePool,
}

pub struct NativeTurnObservation {
    pub native_turn_id: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub terminal: Result<Option<EventType>>,
    pub started_at: Value,
    pub completed_at: Value,
    pub failure: Option<String>,
    pub origin: String,
}

impl NativeTurnService {
    pub fn new(pool: SqlitePool, events: EventIngestService) -> Self {
        Self { pool, events }
    }

    pub async fn observe_turn(
        &self,
        session: &str,
        instance: &str,
        turn: NativeTurnObservation,
    ) -> Result<()> {
        crate::runtime::ControlTarget::resolve(&self.pool, session, Some(instance)).await?;
        let native = &turn.native_turn_id;
        let existing: Option<(String,String)> = sqlx::query_as("SELECT t.turn_id,t.state FROM native_turn_bindings b JOIN turns t ON t.turn_id=b.turn_id WHERE b.session_id=? AND b.client_turn_id=?").bind(session).bind(native).fetch_optional(&self.pool).await?;
        if existing.as_ref().is_some_and(|(_, state)| {
            matches!(state.as_str(), "completed" | "failed" | "interrupted")
        }) {
            return Ok(());
        }
        if existing.is_none() {
            let dispatch = crate::inbox::InboxAssociations::new(self.pool.clone())
                .native_dispatch(session, native)
                .await?;
            let summary = turn
                .input_summary
                .as_deref()
                .or_else(|| dispatch.as_ref().map(|(_, input)| input.as_str()));
            self.events.report_native_fact(session, instance, EventType::TurnStarted, json!({"native_turn_id":native,"input":{"summary":summary.map(|s| s.chars().take(200).collect::<String>())},"metadata":{"native_turn_id":native,"native_started_at":turn.started_at,"observation":turn.origin,"inbox_message_id":dispatch.map(|(id,_)|id)}})).await?;
        }
        crate::inbox::InboxAssociations::new(self.pool.clone())
            .link_native_turn(session, native, Some(instance))
            .await?;
        if let Some(kind) = turn.terminal? {
            if !matches!(
                kind,
                EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted
            ) {
                return Err(Error::Domain("Invalid native terminal fact".into()));
            }
            if let Some(text) = turn.output_summary {
                self.events
                    .report_native_fact(
                        session,
                        instance,
                        EventType::TurnOutput,
                        json!({"native_turn_id":native,"output":{"summary":text}}),
                    )
                    .await?;
            }
            self.events.report_native_fact(session, instance, kind, json!({"native_turn_id":native,"native_completed_at":turn.completed_at,"observation":turn.origin,"failure":{"message":turn.failure}})).await?;
        }
        Ok(())
    }
}
