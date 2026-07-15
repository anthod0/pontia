ALTER TABLE events ADD COLUMN timeline_boundary TEXT;

ALTER TABLE turns ADD COLUMN head_cursor TEXT;
ALTER TABLE turns ADD COLUMN tail_cursor TEXT;
