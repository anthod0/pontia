CREATE TABLE workspace_git_status (
  workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
  repo_root TEXT,
  branch TEXT,
  upstream TEXT,
  ahead INTEGER NOT NULL DEFAULT 0,
  behind INTEGER NOT NULL DEFAULT 0,
  staged_count INTEGER NOT NULL DEFAULT 0,
  unstaged_count INTEGER NOT NULL DEFAULT 0,
  untracked_count INTEGER NOT NULL DEFAULT 0,
  conflicted_count INTEGER NOT NULL DEFAULT 0,
  clean INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL,
  failure TEXT,
  observed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
