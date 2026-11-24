ALTER TABLE agent_profiles ADD COLUMN use_yolo INTEGER NOT NULL DEFAULT 0 CHECK (use_yolo IN (0, 1));
