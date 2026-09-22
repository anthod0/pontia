CREATE TABLE device_bindings (
    device_id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL UNIQUE CHECK (length(trim(account_id)) > 0),
    public_key BLOB NOT NULL UNIQUE CHECK (length(public_key) = 32),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
