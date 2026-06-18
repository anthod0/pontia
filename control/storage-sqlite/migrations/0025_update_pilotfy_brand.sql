UPDATE execution_profiles
SET
    system_prompt_template = replace(system_prompt_template, 'llmparty', 'pilotfy'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE instr(system_prompt_template, 'llmparty') > 0;
