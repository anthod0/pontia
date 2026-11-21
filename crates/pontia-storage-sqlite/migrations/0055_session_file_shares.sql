-- Every earlier share held one file; it keeps that file's UUID under the share prefix.
CREATE TABLE session_files_new (
    file_id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions (session_id),
    share_id TEXT NOT NULL,
    turn_id TEXT REFERENCES turns (turn_id),
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    size INTEGER NOT NULL,
    media_type TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO session_files_new (file_id, session_id, share_id, turn_id, path, name, size, media_type, created_at)
SELECT file_id, session_id, 'share_' || substr(file_id, 6), turn_id, path, name, size, media_type, created_at
FROM session_files;
DROP TABLE session_files;
ALTER TABLE session_files_new RENAME TO session_files;
CREATE INDEX idx_session_files_page ON session_files (session_id, created_at DESC, file_id DESC);
CREATE INDEX idx_session_files_share ON session_files (share_id);

UPDATE events
SET event_type = 'session.files_shared',
    payload = json_object(
        'share_id', 'share_' || substr(json_extract(payload, '$.file_id'), 6),
        'file_ids', json_array(json_extract(payload, '$.file_id'))
    )
WHERE event_type = 'session.file_shared';
