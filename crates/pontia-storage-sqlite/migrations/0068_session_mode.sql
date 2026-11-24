WITH restricted_sessions AS (
    SELECT session_id FROM sessions
    WHERE application_id = 'bot' AND (application_ref NOT GLOB 'helper:*' OR role = 'retrieval')
)
UPDATE events
SET payload = json_set(json_remove(payload, '$.execution_policy'), '$.mode', CASE
    WHEN session_id IN restricted_sessions THEN 'restricted'
    WHEN json_extract(payload, '$.execution_policy.yolo') AND (
        json_extract(payload, '$.execution_policy.disable_skills')
        OR json_extract(payload, '$.execution_policy.disable_mcp')
        OR json_type(payload, '$.execution_policy.tools') = 'array'
        OR json_array_length(payload, '$.execution_policy.disabled_tools') > 0
    ) THEN 'restricted'
    WHEN json_extract(payload, '$.execution_policy.yolo') THEN 'yolo'
    ELSE 'default'
END)
WHERE event_type = 'session.created' AND (
    json_type(payload, '$.execution_policy') IS NOT NULL OR session_id IN restricted_sessions
);
