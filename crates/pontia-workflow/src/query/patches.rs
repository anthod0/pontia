use serde::Serialize;

use super::WorkflowQueryService;
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowActivePatchView {
    pub patch_id: String,
    pub state: String,
    pub base_revision: i64,
    pub request_document_ref: String,
    pub requesting_node_id: String,
    pub requesting_session_id: String,
    pub requesting_turn_id: String,
    pub replanner_session_id: Option<String>,
    pub replanner_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowPatchHistoryView {
    pub patch_id: String,
    pub state: String,
    pub outcome: Option<String>,
    pub base_revision: i64,
    pub result_revision: Option<i64>,
    pub requesting_node_id: String,
    pub requesting_session_id: String,
    pub requesting_turn_id: String,
    pub requesting_runtime_instance_id: String,
    pub replanner_session_id: Option<String>,
    pub replanner_turn_id: Option<String>,
    pub replanner_runtime_instance_id: Option<String>,
    pub added_node_ids: Vec<String>,
    pub retired_node_ids: Vec<String>,
    pub request_document_ref: String,
    pub decision_document_ref: Option<String>,
    pub reason_document_ref: Option<String>,
    pub blocked_draft_ref: Option<String>,
    pub requested_at: String,
    pub planning_at: Option<String>,
    pub resolved_at: Option<String>,
}

impl WorkflowQueryService {
    pub async fn list_workflow_patches(
        &self,
        workflow_id: &str,
    ) -> Result<Option<Vec<WorkflowPatchHistoryView>>> {
        if self.workflows.get_workflow(workflow_id).await?.is_none() {
            return Ok(None);
        }
        let nodes = self.workflows.list_node_history(workflow_id).await?;
        let patches = self.workflows.list_patches(workflow_id).await?;
        Ok(Some(
            patches
                .into_iter()
                .map(|patch| {
                    let changed_revision = patch
                        .result_revision
                        .filter(|revision| *revision > patch.base_revision);
                    let added_node_ids = changed_revision
                        .map(|revision| {
                            nodes
                                .iter()
                                .filter(|node| node.introduced_revision == revision)
                                .map(|node| node.node_id.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    let retired_node_ids = changed_revision
                        .map(|revision| {
                            nodes
                                .iter()
                                .filter(|node| node.retired_revision == Some(revision))
                                .map(|node| node.node_id.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    WorkflowPatchHistoryView {
                        patch_id: patch.patch_id,
                        outcome: matches!(patch.state.as_str(), "applied" | "rejected" | "blocked")
                            .then(|| patch.state.clone()),
                        state: patch.state,
                        base_revision: patch.base_revision,
                        result_revision: patch.result_revision,
                        requesting_node_id: patch.requesting_node_id,
                        requesting_session_id: patch.requesting_session_id,
                        requesting_turn_id: patch.requesting_turn_id,
                        requesting_runtime_instance_id: patch.requesting_runtime_instance_id,
                        replanner_session_id: patch.replanner_session_id,
                        replanner_turn_id: patch.replanner_turn_id,
                        replanner_runtime_instance_id: patch.replanner_runtime_instance_id,
                        added_node_ids,
                        retired_node_ids,
                        request_document_ref: patch.request_document_ref,
                        decision_document_ref: patch.decision_document_ref,
                        reason_document_ref: patch.reason_document_ref,
                        blocked_draft_ref: patch.blocked_draft_ref,
                        requested_at: patch.requested_at,
                        planning_at: patch.planning_at,
                        resolved_at: patch.resolved_at,
                    }
                })
                .collect(),
        ))
    }

    pub(super) async fn active_patch(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowActivePatchView>> {
        Ok(self
            .workflows
            .get_active_patch(workflow_id)
            .await?
            .map(|patch| WorkflowActivePatchView {
                patch_id: patch.patch_id,
                state: patch.state,
                base_revision: patch.base_revision,
                request_document_ref: patch.request_document_ref,
                requesting_node_id: patch.requesting_node_id,
                requesting_session_id: patch.requesting_session_id,
                requesting_turn_id: patch.requesting_turn_id,
                replanner_session_id: patch.replanner_session_id,
                replanner_turn_id: patch.replanner_turn_id,
            }))
    }
}
