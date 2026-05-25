ALTER TABLE graph_work_item_edges ADD COLUMN active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1));

CREATE INDEX idx_graph_work_item_edges_task_active ON graph_work_item_edges(task_id, active, from_work_item_id, to_work_item_id, edge_type);

ALTER TABLE work_item_runtime_projection ADD COLUMN outcome_state TEXT CHECK(outcome_state IN ('succeeded', 'failed', 'blocked', 'cancelled', 'partial', 'unknown'));
ALTER TABLE work_item_runtime_projection ADD COLUMN outcome_reason TEXT;
ALTER TABLE work_item_runtime_projection ADD COLUMN replanned_from_state TEXT;
