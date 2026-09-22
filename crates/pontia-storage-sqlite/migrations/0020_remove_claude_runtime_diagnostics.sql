UPDATE runtime_bindings
SET diagnostics = json_remove(diagnostics, '$.claude_hook_log')
WHERE json_type(diagnostics, '$.claude_hook_log') IS NOT NULL;
