ALTER TABLE bots DROP COLUMN working_on;
ALTER TABLE bots ADD COLUMN awaiting_reply INTEGER NOT NULL DEFAULT 0 CHECK (awaiting_reply IN (0, 1));
ALTER TABLE bots ADD COLUMN awaiting_reply_after INTEGER;
