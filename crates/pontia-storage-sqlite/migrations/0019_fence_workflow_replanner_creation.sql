CREATE UNIQUE INDEX idx_sessions_workflow_replanner_creation_token
    ON sessions(json_extract(metadata, '$.workflow_replanner_creation_token'))
    WHERE json_extract(metadata, '$.workflow_replanner_creation_token') IS NOT NULL;
