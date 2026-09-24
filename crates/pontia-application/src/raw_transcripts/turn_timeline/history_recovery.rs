use pontia_core::{
    Result,
    domain::{TimelineBoundary, TurnState, TurnTopology},
};
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

use super::TurnTimelineService;
use crate::{
    AgentBindingService,
    client_contract::{
        TopologyResolution,
        history::{HistoryRecoveryRequest, TurnHistoryCandidate},
        raw_transcripts::AgentBindingResolveRequest,
    },
};

impl TurnTimelineService {
    pub(crate) async fn try_recover_history(&self, session_id: &str) {
        if let Err(error) = self.recover_history(session_id).await {
            tracing::warn!(
                session_id,
                error = %error,
                diagnostic = "history_recovery_unavailable",
                "native history association recovery unavailable"
            );
        }
    }

    async fn recover_history(&self, session_id: &str) -> Result<()> {
        let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
        else {
            return Ok(());
        };
        let Some(data) = self.clients.data(&binding.client_type) else {
            return Ok(());
        };
        let Some(recoverer) = data.history_recovery() else {
            return Ok(());
        };
        let rows = SqliteTurnRepository::new(self.pool.clone())
            .list_turns(session_id)
            .await?;
        let turns = rows
            .into_iter()
            .map(|turn| {
                Ok(TurnHistoryCandidate {
                    turn_id: turn.turn_id,
                    head_cursor: turn.head_cursor,
                    tail_cursor: turn.tail_cursor,
                    state: turn.state.parse()?,
                    topology: TurnTopology::from_parts(&turn.topology_status, turn.parent_turn_id)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !turns.iter().any(|turn| {
            turn.head_cursor.is_some()
                && (turn.topology == TurnTopology::Unknown
                    || (turn.state == TurnState::Abandoned && turn.tail_cursor.is_none()))
        }) {
            return Ok(());
        }
        let source = data
            .timeline()
            .resolver
            .resolve(&AgentBindingResolveRequest {
                id: binding.id.clone(),
                session_id: binding.session_id.clone(),
                client_type: binding.client_type.clone(),
                client_session_key: binding.client_session_key.clone(),
                client_session_file: binding.client_session_file.clone().map(Into::into),
            })?;
        for recovered in recoverer.recover(HistoryRecoveryRequest {
            source,
            turns: turns.clone(),
        })? {
            let Some(turn) = turns.iter().find(|turn| turn.turn_id == recovered.turn_id) else {
                return Err(pontia_core::Error::Domain(
                    "History recovery returned an unknown Turn".into(),
                ));
            };
            if let Some(cursor) = recovered.tail_cursor {
                if turn.state != TurnState::Abandoned || turn.tail_cursor.is_some() {
                    return Err(pontia_core::Error::Domain(
                        "History recovery requires an unclosed abandoned Turn".into(),
                    ));
                }
                self.events
                    .recover_timeline_boundary(
                        session_id,
                        &turn.turn_id,
                        &binding.client_type,
                        &binding.id,
                        TimelineBoundary::tail(cursor),
                    )
                    .await?;
            }
            let topology = match recovered.topology.resolution {
                TopologyResolution::Root => TurnTopology::Root,
                TopologyResolution::Linked { parent_turn_id } => {
                    TurnTopology::linked(parent_turn_id)
                }
                TopologyResolution::Unknown => {
                    if turn.topology == TurnTopology::Unknown {
                        tracing::warn!(session_id, turn_id = %turn.turn_id,
                            diagnostic = recovered.topology.diagnostic.as_str(),
                            "native history association remains unresolved");
                    }
                    continue;
                }
            };
            if turn.topology == TurnTopology::Unknown {
                self.events
                    .recover_turn_topology(&binding, &turn.turn_id, topology)
                    .await?;
            }
        }
        Ok(())
    }
}
