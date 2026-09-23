use std::collections::HashMap;

use serde::Serialize;

use super::WorkflowQueryService;
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTimelineView {
    pub workflow_id: String,
    pub entries: Vec<WorkflowTimelineEntryView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTimelineEntryView {
    pub fact_kind: String,
    pub source: String,
    pub event_id: String,
    pub event_type: String,
    pub persisted_at: String,
    pub occurred_at: Option<String>,
    pub workflow_sequence: Option<i64>,
    pub agent_event_order: Option<i64>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub node_id: Option<String>,
    pub patch_ids: Vec<String>,
    pub payload: serde_json::Value,
}

impl WorkflowQueryService {
    pub async fn get_workflow_timeline(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowTimelineView>> {
        if self.workflows.get_workflow(workflow_id).await?.is_none() {
            return Ok(None);
        }
        let nodes = self.workflows.list_node_history(workflow_id).await?;
        let patches = self.workflows.list_patches(workflow_id).await?;
        let node_by_session: HashMap<&str, &str> = nodes
            .iter()
            .filter_map(|node| {
                node.session_id
                    .as_deref()
                    .map(|session_id| (session_id, node.node_id.as_str()))
            })
            .collect();
        let workflow_events = self.workflows.list_events(workflow_id).await?;
        let agent_events = self
            .workflows
            .list_workflow_agent_events(workflow_id)
            .await?;
        let mut entries = Vec::with_capacity(workflow_events.len() + agent_events.len());

        for event in workflow_events {
            let payload: serde_json::Value = serde_json::from_str(&event.payload)?;
            let patch_ids = payload
                .get("patch_id")
                .and_then(serde_json::Value::as_str)
                .map(|id| vec![id.to_string()])
                .unwrap_or_default();
            let node_id = payload
                .get("node_id")
                .or_else(|| payload.get("requesting_node_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let session_id = payload
                .get("requesting_session_id")
                .or_else(|| payload.get("replanner_session_id"))
                .or_else(|| payload.get("session_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let turn_id = payload
                .get("requesting_turn_id")
                .or_else(|| payload.get("replanner_turn_id"))
                .or_else(|| payload.get("turn_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            entries.push(WorkflowTimelineEntryView {
                fact_kind: "pontia_orchestration".to_string(),
                source: "pontia_workflow".to_string(),
                event_id: event.event_id,
                event_type: event.event_type,
                persisted_at: event.created_at,
                occurred_at: None,
                workflow_sequence: Some(event.sequence),
                agent_event_order: None,
                session_id,
                turn_id,
                node_id,
                patch_ids,
                payload,
            });
        }
        for event in agent_events {
            let patch_ids = patches
                .iter()
                .filter(|patch| match event.turn_id.as_deref() {
                    Some(turn_id) => {
                        patch.requesting_turn_id == turn_id
                            || patch.replanner_turn_id.as_deref() == Some(turn_id)
                    }
                    None => {
                        patch.requesting_session_id == event.session_id
                            || patch.replanner_session_id.as_deref()
                                == Some(event.session_id.as_str())
                    }
                })
                .map(|patch| patch.patch_id.clone())
                .collect();
            let fact_kind = if matches!(
                event.source.as_str(),
                "agent_client" | "agent_adapter" | "runtime_manager"
            ) {
                "agent_lifecycle"
            } else {
                "pontia_orchestration"
            };
            entries.push(WorkflowTimelineEntryView {
                fact_kind: fact_kind.to_string(),
                source: event.source,
                event_id: event.event_id,
                event_type: event.event_type,
                persisted_at: event.created_at,
                occurred_at: Some(event.occurred_at),
                workflow_sequence: None,
                agent_event_order: Some(event.rowid),
                node_id: node_by_session
                    .get(event.session_id.as_str())
                    .map(|id| (*id).to_string()),
                session_id: Some(event.session_id),
                turn_id: event.turn_id,
                patch_ids,
                payload: serde_json::from_str(&event.payload)?,
            });
        }
        entries.sort_by(|left, right| {
            let left_source = (left.fact_kind == "agent_lifecycle") as u8;
            let right_source = (right.fact_kind == "agent_lifecycle") as u8;
            (&left.persisted_at, left_source, &left.event_id).cmp(&(
                &right.persisted_at,
                right_source,
                &right.event_id,
            ))
        });
        Ok(Some(WorkflowTimelineView {
            workflow_id: workflow_id.to_string(),
            entries,
        }))
    }
}
