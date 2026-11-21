ALTER TABLE sessions ADD COLUMN exit_when_done INTEGER NOT NULL DEFAULT 0
    CHECK (exit_when_done IN (0, 1));
