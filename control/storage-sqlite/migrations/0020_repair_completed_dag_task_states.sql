-- Reconcile DAG task states from the SQLite runtime projection.
-- Some databases may have completed all WorkItem runs while task.state stayed running
-- because aggregation depended on graph-store WorkItem rows instead of the runtime projection.

WITH active_runtime AS (
    SELECT task_id, current_state, optional
    FROM work_item_runtime_projection
    WHERE current_state != 'superseded'
), runtime_counts AS (
    SELECT task_id,
           SUM(CASE WHEN optional = 0 THEN 1 ELSE 0 END) AS required_count
    FROM active_runtime
    GROUP BY task_id
), considered AS (
    SELECT active_runtime.task_id, active_runtime.current_state
    FROM active_runtime
    JOIN runtime_counts ON runtime_counts.task_id = active_runtime.task_id
    WHERE runtime_counts.required_count = 0 OR active_runtime.optional = 0
), aggregate AS (
    SELECT task_id,
           CASE
               WHEN SUM(CASE WHEN current_state NOT IN ('completed', 'replan_anchor') THEN 1 ELSE 0 END) = 0 THEN 'completed'
               WHEN SUM(CASE WHEN current_state = 'failed' THEN 1 ELSE 0 END) > 0 THEN 'failed'
               WHEN SUM(CASE WHEN current_state IN ('blocked', 'needs_input', 'cancelled') THEN 1 ELSE 0 END) > 0 THEN 'blocked'
               ELSE 'running'
           END AS next_state
    FROM considered
    GROUP BY task_id
), changed AS (
    SELECT tasks.task_id, aggregate.next_state
    FROM tasks
    JOIN aggregate ON aggregate.task_id = tasks.task_id
    WHERE aggregate.next_state != 'running'
      AND tasks.state NOT IN ('completed', 'failed', 'cancelled', 'replanning', 'paused')
      AND tasks.state != aggregate.next_state
)
INSERT INTO task_events (event_id, task_id, event_type, payload)
SELECT 'evt_0020_repair_' || next_state || '_' || task_id,
       task_id,
       CASE next_state
           WHEN 'completed' THEN 'task.completed'
           WHEN 'failed' THEN 'task.failed'
           WHEN 'blocked' THEN 'task.blocked'
           ELSE 'task.running'
       END,
       '{"source":"migration_0020_runtime_projection_repair"}'
FROM changed;

WITH active_runtime AS (
    SELECT task_id, current_state, optional
    FROM work_item_runtime_projection
    WHERE current_state != 'superseded'
), runtime_counts AS (
    SELECT task_id,
           SUM(CASE WHEN optional = 0 THEN 1 ELSE 0 END) AS required_count
    FROM active_runtime
    GROUP BY task_id
), considered AS (
    SELECT active_runtime.task_id, active_runtime.current_state
    FROM active_runtime
    JOIN runtime_counts ON runtime_counts.task_id = active_runtime.task_id
    WHERE runtime_counts.required_count = 0 OR active_runtime.optional = 0
), aggregate AS (
    SELECT task_id,
           CASE
               WHEN SUM(CASE WHEN current_state NOT IN ('completed', 'replan_anchor') THEN 1 ELSE 0 END) = 0 THEN 'completed'
               WHEN SUM(CASE WHEN current_state = 'failed' THEN 1 ELSE 0 END) > 0 THEN 'failed'
               WHEN SUM(CASE WHEN current_state IN ('blocked', 'needs_input', 'cancelled') THEN 1 ELSE 0 END) > 0 THEN 'blocked'
               ELSE 'running'
           END AS next_state
    FROM considered
    GROUP BY task_id
)
UPDATE tasks
SET state = (SELECT next_state FROM aggregate WHERE aggregate.task_id = tasks.task_id),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE task_id IN (
    SELECT aggregate.task_id
    FROM aggregate
    WHERE aggregate.next_state != 'running'
      AND tasks.state NOT IN ('completed', 'failed', 'cancelled', 'replanning', 'paused')
      AND tasks.state != aggregate.next_state
);
