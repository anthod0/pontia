ALTER TABLE inbox_messages ADD COLUMN submission_payload TEXT;
ALTER TABLE inbox_messages ADD COLUMN steer_target_turn_id TEXT REFERENCES turns(turn_id);
ALTER TABLE inbox_messages ADD COLUMN retry_of_message_id TEXT REFERENCES inbox_messages(message_id);
CREATE UNIQUE INDEX idx_inbox_messages_retry_of ON inbox_messages(retry_of_message_id)
WHERE retry_of_message_id IS NOT NULL;
