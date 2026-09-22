CREATE TABLE devices (
    device_id TEXT PRIMARY KEY NOT NULL,
    public_key BLOB NOT NULL UNIQUE CHECK (length(public_key) = 32),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE access_keys (
    key_id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(key_id)) > 0),
    secret_hash BLOB NOT NULL UNIQUE CHECK (length(secret_hash) = 32),
    device_id TEXT REFERENCES devices(device_id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
