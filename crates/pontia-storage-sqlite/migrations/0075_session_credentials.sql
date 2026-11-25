ALTER TABLE sessions ADD COLUMN credential_env TEXT;
ALTER TABLE sessions ADD COLUMN credential_sha256 BLOB;
UPDATE sessions SET runtime_environment=json_remove(runtime_environment, '$.PONTIA_BOT_TOOL_TOKEN')
WHERE json_type(runtime_environment, '$.PONTIA_BOT_TOOL_TOKEN') IS NOT NULL;
