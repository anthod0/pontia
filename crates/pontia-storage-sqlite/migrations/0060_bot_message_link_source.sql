ALTER TABLE bot_messages ADD COLUMN link_source TEXT
    CHECK (link_source IS NULL OR (kind = 'user_input' AND link_source IN ('quote', 'bot')));

UPDATE bot_messages SET link_source = 'quote'
WHERE kind = 'user_input' AND id IN (SELECT message_id FROM bot_message_links);
