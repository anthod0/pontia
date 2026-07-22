use super::*;
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

impl ExternalQueryService {
    pub async fn list_turns(&self, session_id: &str) -> Result<Vec<TurnView>> {
        let repository = SqliteTurnRepository::new(self.pool.clone());
        let rows = repository.list_turns(session_id).await?;

        let mut turns = rows
            .into_iter()
            .map(turn_row_to_view)
            .collect::<Result<Vec<_>>>()?;
        for turn in &mut turns {
            self.enrich_turn_view(turn).await?;
        }
        Ok(turns)
    }

    pub async fn get_turn(&self, session_id: &str, turn_id: &str) -> Result<Option<TurnView>> {
        let repository = SqliteTurnRepository::new(self.pool.clone());
        let Some(row) = repository.get_turn(session_id, turn_id).await? else {
            return Ok(None);
        };
        let mut turn = turn_row_to_view(row)?;
        self.enrich_turn_view(&mut turn).await?;
        Ok(Some(turn))
    }

    pub(crate) async fn enrich_turn_view(&self, turn: &mut TurnView) -> Result<()> {
        let repository = SqliteTurnRepository::new(self.pool.clone());
        let rows = repository
            .list_turn_event_enrichment_rows(&turn.session_id, &turn.turn_id)
            .await?;

        for row in rows {
            let event_type = row.event_type;
            let occurred_at = row.occurred_at;
            let payload: Value = serde_json::from_str(&row.payload)?;

            match event_type.as_str() {
                "turn.created" | "turn.queued" | "turn.started" => {
                    if event_type == "turn.started" && turn.started_at.is_none() {
                        turn.started_at = Some(occurred_at.clone());
                    }
                    if turn.input.summary.is_none() {
                        turn.input.summary = nested_string(&payload, &["input", "summary"])
                            .or_else(|| nested_string(&payload, &["input_summary"]));
                    }
                }
                "turn.output" | "turn.completed" => {
                    if event_type == "turn.completed" && turn.state != "completed" {
                        continue;
                    }
                    if event_type == "turn.completed" {
                        turn.completed_at = Some(occurred_at.clone());
                    }
                    if turn.output.summary.is_none() {
                        turn.output.summary = nested_string(&payload, &["output", "summary"])
                            .or_else(|| nested_string(&payload, &["output_summary"]));
                    }
                    if event_type == "turn.completed" {
                        break;
                    }
                }
                "turn.failed" | "turn.interrupted" => {
                    let expected_state = event_type.strip_prefix("turn.").unwrap_or_default();
                    if turn.state != expected_state {
                        continue;
                    }
                    turn.completed_at = Some(occurred_at);
                    if turn.failure.is_none() {
                        turn.failure = nested_string(&payload, &["failure", "message"])
                            .or_else(|| nested_string(&payload, &["message"]));
                    }
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
