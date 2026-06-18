UPDATE execution_profiles
SET supported_client_types = '["pi","claude_code"]',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE profile_id IN ('default', 'planner', 'replanner', 'implementer', 'reviewer', 'tester', 'debugger')
  AND version = '1'
  AND supported_client_types = '["pi","claude_code","generic"]';
