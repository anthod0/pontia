ALTER TABLE bots ADD COLUMN reasoning_effort TEXT;
ALTER TABLE agent_profiles ADD COLUMN reasoning_effort TEXT
    CHECK (reasoning_effort IS NULL OR model IS NOT NULL);
