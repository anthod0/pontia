ALTER TABLE inbox_messages ADD COLUMN branch_target_turn_id TEXT
    REFERENCES turns(turn_id);

CREATE INDEX idx_inbox_messages_branch_target
ON inbox_messages(branch_target_turn_id);
