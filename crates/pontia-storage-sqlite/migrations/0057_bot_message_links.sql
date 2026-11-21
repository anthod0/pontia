CREATE TABLE bot_message_links (
    message_id TEXT NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES bot_messages(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, target_id)
);

CREATE INDEX idx_bot_message_links_target ON bot_message_links(target_id);
