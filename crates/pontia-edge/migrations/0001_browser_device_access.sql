CREATE TABLE browser_device_access (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    secret_hash BLOB NOT NULL,
    device_id TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_browser_device_access_secret_device
    ON browser_device_access (secret_hash, device_id);

CREATE INDEX idx_browser_device_access_expires_at
    ON browser_device_access (expires_at);
