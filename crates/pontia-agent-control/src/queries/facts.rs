use super::ControlQueryService;
use pontia_core::{
    Result,
    domain::{EventType, TurnState},
};
use pontia_storage_sqlite::repositories::events::SqliteEventRepository;
pub use pontia_storage_sqlite::repositories::events::{CompletedTurnWithBoundaries, ReportEvent};
use serde_json::Value;

impl ControlQueryService {
    pub async fn find_by_report_key(
        &self,
        session: &str,
        kind: EventType,
        key: &str,
    ) -> Result<Option<ReportEvent>> {
        SqliteEventRepository::new(self.pool.clone())
            .find_by_report_key(session, kind, key)
            .await
    }

    pub async fn find_turn_by_report_key(
        &self,
        session: &str,
        kind: EventType,
        key: &str,
    ) -> Result<Option<(String, TurnState)>> {
        SqliteEventRepository::new(self.pool.clone())
            .find_turn_by_report_key(session, kind, key)
            .await
    }

    pub async fn latest_event(&self, session: &str, kind: EventType) -> Result<Option<Value>> {
        SqliteEventRepository::new(self.pool.clone())
            .latest_payload_by_type(session, kind)
            .await
    }

    pub async fn list_events_by_type(
        &self,
        session: &str,
        kind: EventType,
        turn: Option<&str>,
    ) -> Result<Vec<Value>> {
        SqliteEventRepository::new(self.pool.clone())
            .list_payloads_by_type(session, kind, turn)
            .await
    }

    pub async fn list_turn_payloads_by_type(
        &self,
        session: &str,
        kind: EventType,
    ) -> Result<Vec<(String, Value)>> {
        SqliteEventRepository::new(self.pool.clone())
            .list_turn_payloads_by_type(session, kind)
            .await
    }

    pub async fn latest_failure_after_event(
        &self,
        session: &str,
        after: EventType,
        failures: &[EventType],
    ) -> Result<Option<String>> {
        SqliteEventRepository::new(self.pool.clone())
            .latest_failure_after(session, after, failures)
            .await
    }

    pub async fn active_turn_started_event(
        &self,
        session: &str,
    ) -> Result<Option<(String, Value)>> {
        SqliteEventRepository::new(self.pool.clone())
            .active_turn_started_payload(session)
            .await
    }

    pub async fn completed_turns_before_with_boundaries(
        &self,
        session: &str,
        target: &str,
    ) -> Result<Vec<CompletedTurnWithBoundaries>> {
        SqliteEventRepository::new(self.pool.clone())
            .completed_turns_before_with_boundaries(session, target)
            .await
    }
}
