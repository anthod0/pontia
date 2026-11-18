ALTER TABLE bot_messages ADD COLUMN status TEXT NOT NULL DEFAULT 'normal'
    CHECK (status = 'normal' OR (status = 'failed' AND kind = 'user_input'));

ALTER TABLE bots ADD COLUMN working_on TEXT;
