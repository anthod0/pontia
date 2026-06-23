#![allow(clippy::too_many_lines)]

pub(super) const LBUG_SCHEMA: &[&str] = &[
    "CREATE NODE TABLE IF NOT EXISTS Task(task_id STRING, title STRING, description STRING, ref STRING, metadata STRING, created_at STRING, updated_at STRING, PRIMARY KEY(task_id));",
    "CREATE NODE TABLE IF NOT EXISTS WorkItem(work_item_id STRING, task_id STRING, title STRING, description STRING, kind STRING, action STRING, execution_profile_id STRING, execution_profile_version STRING, review_policy STRING, execution_policy STRING, escalation_policy STRING, priority INT64, optional_flag BOOL, parallelizable BOOL, acceptance_criteria STRING, active BOOL, ref STRING, metadata STRING, created_at STRING, updated_at STRING, PRIMARY KEY(work_item_id));",
    "CREATE NODE TABLE IF NOT EXISTS Signal(signal_id STRING, task_id STRING, work_item_id STRING, run_id STRING, source_session_id STRING, source STRING, kind STRING, summary STRING, detail STRING, severity STRING, related_refs STRING, state STRING, ref STRING, metadata STRING, created_at STRING, updated_at STRING, PRIMARY KEY(signal_id));",
    "CREATE REL TABLE IF NOT EXISTS HAS_WORK(FROM Task TO WorkItem);",
    "CREATE REL TABLE IF NOT EXISTS HAS_SIGNAL(FROM Task TO Signal);",
    "CREATE REL TABLE IF NOT EXISTS DEPENDS_ON(FROM WorkItem TO WorkItem, edge_id STRING, task_id STRING, ref STRING, metadata STRING, active BOOL, created_at STRING);",
    "CREATE REL TABLE IF NOT EXISTS REVIEWS(FROM WorkItem TO WorkItem, edge_id STRING, task_id STRING, ref STRING, metadata STRING, active BOOL, created_at STRING);",
    "CREATE REL TABLE IF NOT EXISTS SUPERSEDES(FROM WorkItem TO WorkItem, edge_id STRING, task_id STRING, ref STRING, metadata STRING, active BOOL, created_at STRING);",
    "CREATE REL TABLE IF NOT EXISTS CAUSED_BY(FROM WorkItem TO WorkItem, edge_id STRING, task_id STRING, ref STRING, metadata STRING, active BOOL, created_at STRING);",
];
