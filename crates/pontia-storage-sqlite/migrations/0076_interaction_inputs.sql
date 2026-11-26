CREATE TABLE interactions_new (
    interaction_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(session_id),
    turn_id TEXT REFERENCES turns(turn_id),
    type TEXT NOT NULL CHECK (type IN ('choice', 'multi_select', 'text')),
    title TEXT NOT NULL,
    description TEXT,
    options TEXT NOT NULL,
    allows_custom_text INTEGER NOT NULL DEFAULT 0 CHECK (allows_custom_text IN (0, 1)),
    state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'expired')),
    response TEXT,
    requested_at TEXT NOT NULL,
    resolved_at TEXT,
    report_key TEXT,
    authorization_digest TEXT,
    settlement_report_key TEXT,
    settlement_outcome TEXT,
    CHECK ((state = 'resolved' AND response IS NOT NULL AND resolved_at IS NOT NULL)
        OR (state <> 'resolved' AND response IS NULL AND resolved_at IS NULL))
);

INSERT INTO interactions_new (
    interaction_id, session_id, turn_id, type, title, description, options,
    allows_custom_text, state, response, requested_at, resolved_at, report_key,
    authorization_digest, settlement_report_key, settlement_outcome
)
SELECT interaction_id, session_id, turn_id, type, title, description, options,
       0, state,
       CASE WHEN response_option_key IS NULL THEN NULL
            ELSE json_object('option_key', response_option_key) END,
       requested_at, resolved_at, report_key, authorization_digest,
       settlement_report_key, settlement_outcome
FROM interactions;

DROP TABLE interactions;
ALTER TABLE interactions_new RENAME TO interactions;

CREATE UNIQUE INDEX idx_interactions_report_key ON interactions(session_id, report_key) WHERE report_key IS NOT NULL;
CREATE UNIQUE INDEX idx_interactions_settlement_key ON interactions(session_id, settlement_report_key) WHERE settlement_report_key IS NOT NULL;
CREATE INDEX idx_interactions_session_order ON interactions(session_id, requested_at, interaction_id);
CREATE INDEX idx_interactions_session_state_order ON interactions(session_id, state, requested_at, interaction_id);
