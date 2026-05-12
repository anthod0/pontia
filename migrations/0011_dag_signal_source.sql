ALTER TABLE dag_signals ADD COLUMN source TEXT NOT NULL DEFAULT 'agent' CHECK(source IN ('agent', 'human', 'system'));
