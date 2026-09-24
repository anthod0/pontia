use super::{
    TurnTimelineService, TurnTimelineServiceError,
    source::{classify_adapter_error, classify_reader_error},
};
use crate::AgentBinding;
use crate::client_contract::raw_transcripts::{
    ResolvedAgentBinding, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TurnTimelineRange, TurnTimelineReadRequest,
};
use pontia_core::{Error, domain::TimelineBoundary};
use pontia_storage_sqlite::models::turns::TurnRow;
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

impl TurnTimelineService {
    pub(super) async fn recover_range(
        &self,
        binding: &AgentBinding,
        source: &ResolvedAgentBinding,
        turn: &TurnRow,
        active: bool,
        is_first: bool,
    ) -> Result<TurnRow, TurnTimelineServiceError> {
        let mut head = turn.head_cursor.clone();
        let mut tail = turn.tail_cursor.clone();
        let native: Option<String> = sqlx::query_scalar(
            "SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?",
        )
        .bind(&binding.session_id)
        .bind(&turn.turn_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::from)?;
        let native =
            native.ok_or_else(|| TurnTimelineServiceError::NativeAssociationUnavailable {
                turn_id: turn.turn_id.clone(),
            })?;
        let boundaries = self
            .clients
            .boundaries(&binding.client_type)
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        for kind in [
            TimelineBoundaryCaptureKind::Head,
            TimelineBoundaryCaptureKind::Tail,
        ] {
            let slot = match kind {
                TimelineBoundaryCaptureKind::Head => &mut head,
                TimelineBoundaryCaptureKind::Tail => &mut tail,
            };
            if slot.is_some() || (active && kind == TimelineBoundaryCaptureKind::Tail) {
                continue;
            }
            *slot = Some(
                boundaries
                    .capturer
                    .capture_boundary(TimelineBoundaryCaptureRequest {
                        source: source.clone(),
                        kind,
                        native_entry_anchor: Some(native.clone()),
                        allow_missing_native_entry_anchor: false,
                    })
                    .map_err(classify_adapter_error)?
                    .cursor,
            );
        }
        // Verify semantic anchors and actual readability before recording recovered locators.
        self.clients
            .timeline(&binding.client_type)
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?
            .reader
            .read_turn_ranges(TurnTimelineReadRequest {
                source: source.clone(),
                ranges: vec![TurnTimelineRange {
                    turn_id: turn.turn_id.clone(),
                    is_first_session_turn: is_first,
                    head_cursor: head.clone().expect("captured head"),
                    tail_cursor: tail.clone(),
                }],
            })
            .map_err(classify_reader_error)?;
        if turn.head_cursor.is_none() {
            self.events
                .recover_timeline_boundary(
                    &binding.session_id,
                    &turn.turn_id,
                    &binding.client_type,
                    &binding.id,
                    TimelineBoundary::head(head.clone().expect("captured head")),
                )
                .await?;
        }
        if turn.tail_cursor.is_none()
            && let Some(tail) = &tail
        {
            self.events
                .recover_timeline_boundary(
                    &binding.session_id,
                    &turn.turn_id,
                    &binding.client_type,
                    &binding.id,
                    TimelineBoundary::tail(tail.clone()),
                )
                .await?;
        }
        // Concurrent recovery uses the first committed capture's physical limits.
        SqliteTurnRepository::new(self.pool.clone())
            .get_turn(&binding.session_id, &turn.turn_id)
            .await?
            .ok_or(TurnTimelineServiceError::TurnNotFound)
    }
}
