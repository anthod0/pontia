CREATE TABLE bot_memories (
    id TEXT PRIMARY KEY NOT NULL,

    kind TEXT NOT NULL CHECK (kind IN (
        'fact', 'decision', 'event', 'lesson', 'observation'
    )),

    content TEXT NOT NULL CHECK (length(trim(content)) > 0),

    keywords TEXT NOT NULL
        CHECK (
            json_valid(keywords)
            AND json_type(keywords) = 'array'
            AND json_array_length(keywords) > 0
        ),

    workspace_id TEXT,
    session_id TEXT,

    archived INTEGER NOT NULL DEFAULT 0
        CHECK (archived IN (0, 1)),

    importance TEXT NOT NULL DEFAULT 'normal'
        CHECK (importance IN ('low', 'normal', 'high')),

    occurred_at TEXT,

    sources TEXT,

    created_by TEXT NOT NULL
        CHECK (created_by IN ('bot', 'dreaming')),

    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_read_at TEXT
);

CREATE TABLE bot_memory_edges (
    memory_a_id TEXT NOT NULL REFERENCES bot_memories(id),
    memory_b_id TEXT NOT NULL REFERENCES bot_memories(id),

    created_at TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    PRIMARY KEY (memory_a_id, memory_b_id),
    CHECK (memory_a_id < memory_b_id)
);

CREATE INDEX idx_bot_memory_edges_b
    ON bot_memory_edges(memory_b_id);

CREATE INDEX idx_bot_memories_active
    ON bot_memories(workspace_id, kind, updated_at DESC)
    WHERE archived = 0;

CREATE INDEX idx_bot_memories_session
    ON bot_memories(session_id)
    WHERE session_id IS NOT NULL;

CREATE VIRTUAL TABLE bot_memory_keywords USING fts5(keywords, tokenize = 'ascii');
