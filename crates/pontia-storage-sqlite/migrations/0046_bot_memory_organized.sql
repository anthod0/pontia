ALTER TABLE bot_memories ADD COLUMN organized INTEGER NOT NULL DEFAULT 0 CHECK (organized IN (0, 1));
CREATE INDEX idx_bot_memories_pending ON bot_memories(id) WHERE archived = 0 AND organized = 0;
