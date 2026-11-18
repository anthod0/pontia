UPDATE execution_profiles
SET supported_client_types = '["pi","codex","claude-agent","acp"]'
WHERE profile_id = 'default' AND version = '2';
