-- Add proposal revision metadata and the superseded state without mutating the
-- historical 0009 migration/check constraint.

CREATE TABLE dag_proposals_new (
    proposal_id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('initial_dag', 'patch')),
    state TEXT NOT NULL CHECK(state IN ('proposed', 'validated', 'rejected', 'applied', 'superseded')),
    summary TEXT NOT NULL,
    proposal_json TEXT NOT NULL,
    validation_json TEXT NOT NULL DEFAULT '{}',
    created_by_session_id TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    supersedes_proposal_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(task_id) REFERENCES tasks(task_id),
    FOREIGN KEY(supersedes_proposal_id) REFERENCES dag_proposals_new(proposal_id)
);

INSERT INTO dag_proposals_new (
    proposal_id, task_id, mode, state, summary, proposal_json, validation_json,
    created_by_session_id, revision, supersedes_proposal_id, created_at, updated_at
)
SELECT
    proposal_id,
    task_id,
    mode,
    state,
    summary,
    proposal_json,
    validation_json,
    created_by_session_id,
    ROW_NUMBER() OVER (PARTITION BY task_id ORDER BY created_at ASC, proposal_id ASC) AS revision,
    NULL,
    created_at,
    updated_at
FROM dag_proposals;

DROP TABLE dag_proposals;
ALTER TABLE dag_proposals_new RENAME TO dag_proposals;

CREATE INDEX idx_dag_proposals_task ON dag_proposals(task_id, created_at, proposal_id);
CREATE INDEX idx_dag_proposals_task_revision ON dag_proposals(task_id, revision);
CREATE INDEX idx_dag_proposals_task_state ON dag_proposals(task_id, state, revision);
