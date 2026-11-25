UPDATE sessions
SET metadata = json_remove(metadata, '$.codex_environment')
WHERE json_type(metadata, '$.codex_environment') IS NOT NULL;
