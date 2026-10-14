-- Event shape and Turn topology are validated by the shared ingestion boundary
-- and domain projection before persistence.
DROP TRIGGER turn_events_require_turn_identity;
DROP TRIGGER turns_validate_topology_on_insert;
DROP TRIGGER turns_validate_topology_on_update;
DROP TRIGGER turns_preserve_resolved_topology;
DROP TRIGGER turns_validate_linked_parent_on_insert;
DROP TRIGGER turns_validate_linked_parent_on_update;
DROP TRIGGER turn_events_validate_linked_parent;
DROP TRIGGER turn_events_validate_topology;

-- Interaction options are validated and JSON-serialized by the application.
CREATE TABLE interactions_new (
    interaction_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(session_id),
    turn_id TEXT REFERENCES turns(turn_id),
    type TEXT NOT NULL CHECK (type = 'choice'),
    title TEXT NOT NULL,
    description TEXT,
    options TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'expired')),
    response_option_key TEXT,
    requested_at TEXT NOT NULL,
    resolved_at TEXT,
    CHECK ((state = 'resolved' AND response_option_key IS NOT NULL AND resolved_at IS NOT NULL)
        OR (state <> 'resolved' AND response_option_key IS NULL AND resolved_at IS NULL))
);

INSERT INTO interactions_new (
    interaction_id, session_id, turn_id, type, title, description, options,
    state, response_option_key, requested_at, resolved_at
)
SELECT interaction_id, session_id, turn_id, type, title, description, options,
       state, response_option_key, requested_at, resolved_at
FROM interactions;

DROP TABLE interactions;
ALTER TABLE interactions_new RENAME TO interactions;
CREATE INDEX idx_interactions_session_order ON interactions(session_id, requested_at, interaction_id);
CREATE INDEX idx_interactions_session_state_order ON interactions(session_id, state, requested_at, interaction_id);
