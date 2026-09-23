use super::InboxCommandService;
use crate::control::InputReceipt;
use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};
use pontia_storage_sqlite::repositories::inbox::SqliteInboxRepository;

impl InboxCommandService {
    pub(crate) async fn record_receipt(
        &self,
        session: &str,
        message: &str,
        receipt: &InputReceipt,
    ) -> Result<()> {
        let (Some(native), Some(_)) = (&receipt.native_turn_id, &receipt.runtime_instance_id)
        else {
            return Ok(());
        };
        sqlx::query("UPDATE inbox_messages SET metadata=json_set(CASE WHEN json_type(metadata)='object' THEN metadata ELSE '{}' END,'$.codex_turn_id',?) WHERE message_id=? AND session_id=? AND EXISTS (SELECT 1 FROM runtime_bindings WHERE session_id=? AND runtime_instance_id=?)")
            .bind(native).bind(message).bind(session).bind(session).bind(&receipt.runtime_instance_id).execute(&self.pool).await?;
        self.link_native_turn(session, native, receipt.runtime_instance_id.as_deref())
            .await
    }

    pub(crate) async fn native_dispatch(
        &self,
        session: &str,
        native: &str,
    ) -> Result<Option<(String, String)>> {
        Ok(sqlx::query_as("SELECT message_id,input_summary FROM inbox_messages WHERE session_id=? AND json_extract(metadata,'$.codex_turn_id')=? ORDER BY created_at LIMIT 1")
            .bind(session).bind(native).fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn link_native_turn(
        &self,
        session: &str,
        native: &str,
        instance: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE inbox_messages SET turn_id=(SELECT t.turn_id FROM native_turn_bindings b JOIN turns t ON t.turn_id=b.turn_id AND t.session_id=b.session_id WHERE b.session_id=? AND b.client_turn_id=?) WHERE session_id=? AND json_extract(metadata,'$.codex_turn_id')=? AND turn_id IS NULL AND (? IS NULL OR EXISTS (SELECT 1 FROM runtime_bindings WHERE session_id=? AND runtime_instance_id=?))")
            .bind(session).bind(native).bind(session).bind(native).bind(instance).bind(session).bind(instance).execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn observe_committed(&self, event: &DomainEvent) -> Result<()> {
        if matches!(
            event.event_type,
            EventType::TurnStarted | EventType::SessionExited | EventType::SessionError
        ) {
            self.event_ingest
                .inbox_scheduler()
                .finish_initial(&event.session_id);
        }
        if event.event_type == EventType::TurnStarted
            && let Some(turn) = &event.turn_id
        {
            if let Some(message) = event
                .payload
                .pointer("/metadata/inbox_message_id")
                .or_else(|| event.payload.pointer("/input/inbox_message_id"))
                .and_then(serde_json::Value::as_str)
            {
                SqliteInboxRepository::new(self.pool.clone())
                    .link_started_turn(&event.session_id, message, turn)
                    .await?;
            }
            if let Some(native) = event
                .payload
                .get("native_turn_id")
                .and_then(serde_json::Value::as_str)
            {
                self.link_native_turn(
                    &event.session_id,
                    native,
                    event.payload["runtime_instance_id"].as_str(),
                )
                .await?;
            }
        }
        if matches!(
            event.event_type,
            EventType::SessionReady
                | EventType::TurnStarted
                | EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted
        ) {
            self.notify_available(&event.session_id);
        }
        Ok(())
    }
}
