ALTER TABLE execution_profiles
ADD COLUMN agent_kind TEXT NOT NULL DEFAULT 'executor';

UPDATE execution_profiles
SET agent_kind = 'planner',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE profile_id IN ('planner', 'replanner');
