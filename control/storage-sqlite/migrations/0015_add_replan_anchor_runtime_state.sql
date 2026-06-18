PRAGMA foreign_keys = OFF;

CREATE TABLE work_item_runtime_projection_new (
    work_item_id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    current_run_id TEXT,
    current_state TEXT NOT NULL CHECK(current_state IN ('pending', 'ready', 'blocked', 'running', 'completed', 'failed', 'needs_input', 'cancelled', 'superseded', 'replan_anchor')),
    current_attempt INTEGER NOT NULL DEFAULT 0,
    ready_at TEXT,
    blocked_reason TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    optional INTEGER NOT NULL DEFAULT 0 CHECK(optional IN (0, 1)),
    parallelizable INTEGER NOT NULL DEFAULT 1 CHECK(parallelizable IN (0, 1)),
    session_id TEXT,
    turn_id TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(work_item_id) REFERENCES work_items(work_item_id),
    FOREIGN KEY(task_id) REFERENCES tasks(task_id),
    FOREIGN KEY(current_run_id) REFERENCES work_item_runs(run_id),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id),
    FOREIGN KEY(turn_id) REFERENCES turns(turn_id)
);

INSERT INTO work_item_runtime_projection_new (
    work_item_id, task_id, current_run_id, current_state, current_attempt, ready_at,
    blocked_reason, retry_count, max_retries, priority, optional, parallelizable,
    session_id, turn_id, updated_at
)
SELECT
    work_item_id, task_id, current_run_id, current_state, current_attempt, ready_at,
    blocked_reason, retry_count, max_retries, priority, optional, parallelizable,
    session_id, turn_id, updated_at
FROM work_item_runtime_projection;

DROP TABLE work_item_runtime_projection;
ALTER TABLE work_item_runtime_projection_new RENAME TO work_item_runtime_projection;
CREATE INDEX idx_work_item_runtime_ready ON work_item_runtime_projection(task_id, current_state, priority, ready_at, work_item_id);

PRAGMA foreign_keys = ON;
