ALTER TABLE agent_bindings ADD COLUMN client_session_file TEXT;

UPDATE agent_bindings
SET client_session_file = CASE
    WHEN json_type(metadata, '$.client_session_file') = 'text'
         AND trim(json_extract(metadata, '$.client_session_file')) <> ''
    THEN json_extract(metadata, '$.client_session_file')
    ELSE NULL
END
WHERE json_valid(metadata);
