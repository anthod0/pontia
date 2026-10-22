CREATE TABLE agent_clients (
    client_type TEXT NOT NULL,
    client_key TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    config_overrides TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(config_overrides) AND json_type(config_overrides) = 'object'),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (client_type, client_key)
);
