-- Associate every DAG proposal with the planning turn that created it.
-- Backfill existing proposals from their creator session and task-scoped planning turns.

PRAGMA foreign_keys=OFF;

CREATE TABLE dag_proposals_new (
    proposal_id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('initial_dag', 'patch')),
    state TEXT NOT NULL CHECK(state IN ('proposed', 'validated', 'rejected', 'applied', 'superseded')),
    summary TEXT NOT NULL,
    proposal_json TEXT NOT NULL,
    validation_json TEXT NOT NULL DEFAULT '{}',
    created_by_session_id TEXT,
    created_by_turn_id TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    supersedes_proposal_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY(task_id) REFERENCES tasks(task_id),
    FOREIGN KEY(created_by_turn_id) REFERENCES turns(turn_id),
    FOREIGN KEY(supersedes_proposal_id) REFERENCES dag_proposals_new(proposal_id)
);

CREATE TEMP TABLE dag_proposal_turn_backfill (
    proposal_id TEXT PRIMARY KEY NOT NULL,
    turn_id TEXT NOT NULL
);

INSERT INTO dag_proposal_turn_backfill (proposal_id, turn_id)
SELECT proposal_id, turn_id
FROM (
    SELECT
        p.proposal_id,
        t.turn_id,
        ROW_NUMBER() OVER (
            PARTITION BY p.proposal_id
            ORDER BY
                CASE WHEN t.created_at <= p.created_at THEN 0 ELSE 1 END,
                CASE WHEN t.created_at <= p.created_at THEN t.created_at END DESC,
                CASE WHEN t.created_at > p.created_at THEN t.created_at END ASC,
                t.turn_id DESC
        ) AS rn
    FROM dag_proposals p
    JOIN turns t ON t.session_id = p.created_by_session_id
      AND json_extract(t.metadata, '$.dag_managed') = 1
      AND json_extract(t.metadata, '$.dag_planning_role') IS NOT NULL
      AND json_extract(t.metadata, '$.task_id') = p.task_id
)
WHERE rn = 1;

INSERT INTO dag_proposals_new (
    proposal_id, task_id, mode, state, summary, proposal_json, validation_json,
    created_by_session_id, created_by_turn_id, revision, supersedes_proposal_id,
    created_at, updated_at
)
SELECT
    p.proposal_id,
    p.task_id,
    p.mode,
    p.state,
    p.summary,
    p.proposal_json,
    p.validation_json,
    p.created_by_session_id,
    (SELECT b.turn_id FROM dag_proposal_turn_backfill b WHERE b.proposal_id = p.proposal_id),
    p.revision,
    p.supersedes_proposal_id,
    p.created_at,
    p.updated_at
FROM dag_proposals p;

DROP TABLE dag_proposal_turn_backfill;

DROP TABLE dag_proposals;
ALTER TABLE dag_proposals_new RENAME TO dag_proposals;

CREATE INDEX idx_dag_proposals_task ON dag_proposals(task_id, created_at, proposal_id);
CREATE INDEX idx_dag_proposals_task_revision ON dag_proposals(task_id, revision);
CREATE INDEX idx_dag_proposals_task_state ON dag_proposals(task_id, state, revision);
CREATE INDEX idx_dag_proposals_created_by_turn ON dag_proposals(created_by_turn_id, created_at, proposal_id);

PRAGMA foreign_keys=ON;
