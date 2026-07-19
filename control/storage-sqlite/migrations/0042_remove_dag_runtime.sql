DROP TABLE IF EXISTS dag_signals;
DROP TABLE IF EXISTS work_item_runtime_projection;
DROP TABLE IF EXISTS work_item_runs;
DROP TABLE IF EXISTS work_item_edges;
DROP TABLE IF EXISTS work_items;
DROP TABLE IF EXISTS dag_proposals;
DROP TABLE IF EXISTS graph_signals;
DROP TABLE IF EXISTS graph_work_item_edges;
DROP TABLE IF EXISTS graph_work_items;
DROP TABLE IF EXISTS graph_tasks;

DELETE FROM execution_profiles
WHERE profile_id IN ('planner', 'replanner', 'implementer', 'reviewer', 'tester', 'debugger')
  AND json_extract(metadata, '$.builtin') = 1;

UPDATE execution_profiles
SET agent_kind = 'executor',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE agent_kind = 'planner';

UPDATE execution_profiles
SET description = 'General coding agent execution template.',
    system_prompt_template = 'You are a coding agent. Follow the assigned task and report concise results.',
    turn_prompt_template = '{{input}}',
    default_session_role = 'General coding agent',
    default_session_description = 'Executes coding tasks.',
    expected_output_schema = 'free_text',
    artifact_contract = '{}',
    default_execution_policy = '{}',
    default_review_policy = '{}',
    agent_kind = 'executor',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE profile_id = 'default'
  AND version = '1'
  AND json_extract(metadata, '$.builtin') = 1;
