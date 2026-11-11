INSERT INTO execution_profiles (
    profile_id, version, name, description, supported_client_types,
    system_prompt_template, metadata
) VALUES (
    'default', '2', 'Default', 'Pontia environment and file delivery instructions.',
    '["pi","codex","claude-agent"]',
    'You are running in a Pontia Session. Use `pontia send-file <path>` to offer files for download in the Dashboard; see `pontia send-file -h` for usage. Your Session identity is provided automatically. Files are read at download time, so keep shared files available.',
    '{"builtin":true}'
);
