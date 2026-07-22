-- Event ordering uses SQLite rowid cursors. The optional producer-supplied
-- sequence number is no longer accepted or used.
ALTER TABLE events DROP COLUMN seq;
