UPDATE execution_profiles
SET supported_client_types = '["pi"]'
WHERE supported_client_types = '["pi","claude_code"]';
