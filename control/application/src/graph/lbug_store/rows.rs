use lbug::Value as LbugValue;

use pontia_core::error::Result;

use super::values::{
    expect_bool, expect_i64, expect_string, optional_json, optional_string, parse_json_value,
};
use super::{GraphEdgeKind, SignalNode, TaskNode, WorkItemEdgeRecord, WorkItemNode};

pub(super) fn row_to_task(row: Vec<LbugValue>) -> Result<TaskNode> {
    Ok(TaskNode {
        task_id: expect_string(&row[0])?,
        title: expect_string(&row[1])?,
        description: expect_string(&row[2])?,
        ref_: optional_string(&row[3])?,
        metadata: parse_json_value(&row[4])?,
        created_at: expect_string(&row[5])?,
        updated_at: expect_string(&row[6])?,
    })
}

pub(super) fn row_to_work_item(row: Vec<LbugValue>) -> Result<WorkItemNode> {
    Ok(WorkItemNode {
        work_item_id: expect_string(&row[0])?,
        task_id: expect_string(&row[1])?,
        title: expect_string(&row[2])?,
        description: expect_string(&row[3])?,
        kind: expect_string(&row[4])?,
        action: expect_string(&row[5])?,
        execution_profile_id: expect_string(&row[6])?,
        execution_profile_version: optional_string(&row[7])?,
        review_policy: optional_json(&row[8])?,
        execution_policy: optional_json(&row[9])?,
        escalation_policy: optional_json(&row[10])?,
        priority: expect_i64(&row[11])?,
        optional: expect_bool(&row[12])?,
        parallelizable: expect_bool(&row[13])?,
        acceptance_criteria: parse_json_value(&row[14])?,
        active: expect_bool(&row[15])?,
        ref_: optional_string(&row[16])?,
        metadata: parse_json_value(&row[17])?,
        created_at: expect_string(&row[18])?,
        updated_at: expect_string(&row[19])?,
    })
}

pub(super) fn row_to_edge(
    row: Vec<LbugValue>,
    edge_type: GraphEdgeKind,
) -> Result<WorkItemEdgeRecord> {
    Ok(WorkItemEdgeRecord {
        edge_id: expect_string(&row[0])?,
        task_id: expect_string(&row[1])?,
        from_work_item_id: expect_string(&row[2])?,
        to_work_item_id: expect_string(&row[3])?,
        edge_type,
        ref_: optional_string(&row[4])?,
        metadata: parse_json_value(&row[5])?,
        created_at: expect_string(&row[6])?,
    })
}

pub(super) fn row_to_signal(row: Vec<LbugValue>) -> Result<SignalNode> {
    Ok(SignalNode {
        signal_id: expect_string(&row[0])?,
        task_id: expect_string(&row[1])?,
        work_item_id: optional_string(&row[2])?,
        run_id: optional_string(&row[3])?,
        source_session_id: optional_string(&row[4])?,
        source: expect_string(&row[5])?,
        kind: expect_string(&row[6])?,
        summary: expect_string(&row[7])?,
        detail: optional_string(&row[8])?,
        severity: expect_string(&row[9])?,
        related_refs: parse_json_value(&row[10])?,
        state: expect_string(&row[11])?,
        ref_: optional_string(&row[12])?,
        metadata: parse_json_value(&row[13])?,
        created_at: expect_string(&row[14])?,
        updated_at: expect_string(&row[15])?,
    })
}
