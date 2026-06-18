UPDATE execution_profiles
SET
    system_prompt_template = replace(system_prompt_template, 'pilotfy', 'pontia'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE instr(system_prompt_template, 'pilotfy') > 0;
