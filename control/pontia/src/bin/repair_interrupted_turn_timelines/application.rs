use sqlx::SqlitePool;

use super::{RepairError, planning::RepairCandidate};

pub(super) async fn apply_candidates(
    pool: &SqlitePool,
    candidates: &[RepairCandidate],
) -> Result<usize, RepairError> {
    let mut transaction = pool.begin().await?;
    for candidate in candidates {
        let cursor = candidate
            .proposed_tail_cursor
            .as_deref()
            .ok_or("repairable candidate has no proposed tail cursor")?;
        let timeline_boundary = serde_json::to_string(
            candidate
                .proposed_event_timeline_boundary
                .as_ref()
                .ok_or("repairable candidate has no proposed event boundary")?,
        )?;
        let event_result = sqlx::query(
            "UPDATE events SET timeline_boundary = ? WHERE event_id = ? AND event_type = 'turn.interrupted' AND timeline_boundary IS NULL",
        )
        .bind(timeline_boundary)
        .bind(&candidate.event_id)
        .execute(&mut *transaction)
        .await?;
        if event_result.rows_affected() != 1 {
            return Err(format!(
                "event {} changed after validation; rolling back",
                candidate.event_id
            )
            .into());
        }
        let turn_result = sqlx::query(
            "UPDATE turns SET tail_cursor = ? WHERE turn_id = ? AND session_id = ? AND state = 'interrupted' AND tail_cursor IS NULL",
        )
        .bind(cursor)
        .bind(&candidate.turn_id)
        .bind(&candidate.session_id)
        .execute(&mut *transaction)
        .await?;
        if turn_result.rows_affected() != 1 {
            return Err(format!(
                "turn {} changed after validation; rolling back",
                candidate.turn_id
            )
            .into());
        }
    }
    transaction.commit().await?;
    Ok(candidates.len())
}
