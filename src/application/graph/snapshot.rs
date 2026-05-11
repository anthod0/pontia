use serde_json::json;

use super::{ProvenanceEdge, ProvenanceNode, TaskProvenance};

#[derive(Debug)]
pub(super) struct TaskGraphSnapshot {
    pub(super) task_id: String,
    pub(super) task_input: String,
    pub(super) task_created_at: String,
    pub(super) task_updated_at: String,
    pub(super) decisions: Vec<GraphDecision>,
}

#[derive(Debug)]
pub(super) struct GraphDecision {
    pub(super) decision_id: String,
    pub(super) status: String,
    pub(super) reason: String,
    pub(super) confidence: f64,
    pub(super) created_at: String,
    pub(super) evidence: Vec<GraphEvidence>,
}

#[derive(Debug)]
pub(super) struct GraphEvidence {
    pub(super) evidence_id: String,
    pub(super) kind: String,
    pub(super) reference: String,
    pub(super) summary: String,
}

pub(super) fn snapshot_to_provenance(snapshot: TaskGraphSnapshot) -> TaskProvenance {
    let mut nodes = vec![ProvenanceNode {
        id: snapshot.task_id.clone(),
        kind: "Task".to_string(),
        properties: json!({
            "title": task_title(&snapshot.task_input),
            "description": snapshot.task_input,
            "ref": format!("sqlite:task:{}", snapshot.task_id),
            "created_at": snapshot.task_created_at,
            "updated_at": snapshot.task_updated_at
        }),
    }];
    let mut edges = Vec::new();

    if !snapshot.decisions.is_empty() {
        nodes.push(ProvenanceNode {
            id: "agent_planner".to_string(),
            kind: "Agent".to_string(),
            properties: json!({
                "name": "Task Planner",
                "role": "planner",
                "capabilities": "[\"workspace_routing\",\"task_planning\"]",
                "availability": "available",
                "ref": "internal:planner",
                "created_at": "",
                "updated_at": ""
            }),
        });
    }

    for decision in snapshot.decisions {
        let work_item_id = format!("wi_{}", decision.decision_id);
        let signal_id = format!("sig_{}", decision.decision_id);
        nodes.push(ProvenanceNode {
            id: work_item_id.clone(),
            kind: "WorkItem".to_string(),
            properties: json!({
                "title": "Plan task",
                "description": decision.reason,
                "kind": "planning",
                "planning_state": "active",
                "execution_state": decision_execution_state(&decision.status),
                "execution_ref": "",
                "created_at": decision.created_at,
                "updated_at": decision.created_at
            }),
        });
        edges.push(ProvenanceEdge {
            from: snapshot.task_id.clone(),
            to: work_item_id.clone(),
            kind: "HAS_WORK".to_string(),
            properties: json!({}),
        });
        edges.push(ProvenanceEdge {
            from: work_item_id.clone(),
            to: "agent_planner".to_string(),
            kind: "ASSIGNED_TO".to_string(),
            properties: json!({}),
        });

        nodes.push(ProvenanceNode {
            id: signal_id.clone(),
            kind: "Signal".to_string(),
            properties: json!({
                "source_type": "agent",
                "kind": decision_signal_kind(&decision.status),
                "summary": decision.reason,
                "detail": format!("planner status: {}; confidence: {}", decision.status, decision.confidence),
                "origin_ref": format!("sqlite:task:{}", snapshot.task_id),
                "created_at": decision.created_at
            }),
        });
        edges.push(ProvenanceEdge {
            from: snapshot.task_id.clone(),
            to: signal_id.clone(),
            kind: "HAS_SIGNAL".to_string(),
            properties: json!({}),
        });
        edges.push(ProvenanceEdge {
            from: "agent_planner".to_string(),
            to: signal_id.clone(),
            kind: "EMITS".to_string(),
            properties: json!({}),
        });

        for evidence in decision.evidence {
            let artifact_id = format!("art_{}", evidence.evidence_id);
            nodes.push(ProvenanceNode {
                id: artifact_id.clone(),
                kind: "Artifact".to_string(),
                properties: json!({
                    "kind": evidence.kind,
                    "name": evidence.evidence_id,
                    "summary": evidence.summary,
                    "availability": "available",
                    "ref": evidence.reference,
                    "created_at": "",
                    "updated_at": ""
                }),
            });
            edges.push(ProvenanceEdge {
                from: work_item_id.clone(),
                to: artifact_id.clone(),
                kind: "REQUIRES".to_string(),
                properties: json!({}),
            });
            edges.push(ProvenanceEdge {
                from: signal_id.clone(),
                to: artifact_id,
                kind: "SUPPORTED_BY".to_string(),
                properties: json!({}),
            });
        }
    }

    TaskProvenance { nodes, edges }
}

pub(super) fn task_title(input: &str) -> String {
    input
        .lines()
        .next()
        .unwrap_or(input)
        .chars()
        .take(80)
        .collect()
}

pub(super) fn decision_execution_state(status: &str) -> &'static str {
    match status {
        "failed" => "failed",
        _ => "completed",
    }
}

pub(super) fn decision_signal_kind(status: &str) -> &'static str {
    match status {
        "needs_input" => "constraint",
        "failed" => "failure",
        _ => "finding",
    }
}
