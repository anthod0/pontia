CREATE TABLE interactions (
    interaction_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(session_id),
    turn_id TEXT REFERENCES turns(turn_id),
    type TEXT NOT NULL CHECK (type = 'choice'),
    title TEXT NOT NULL,
    description TEXT,
    options TEXT NOT NULL CHECK (json_valid(options)),
    state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'expired')),
    response_option_key TEXT,
    requested_at TEXT NOT NULL,
    resolved_at TEXT,
    CHECK ((state = 'resolved' AND response_option_key IS NOT NULL AND resolved_at IS NOT NULL)
        OR (state <> 'resolved' AND response_option_key IS NULL AND resolved_at IS NULL))
);
CREATE INDEX idx_interactions_session_order ON interactions(session_id, requested_at, interaction_id);
CREATE INDEX idx_interactions_session_state_order ON interactions(session_id, state, requested_at, interaction_id);
