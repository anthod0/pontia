DROP INDEX IF EXISTS idx_graph_signals_work_item;
DROP INDEX IF EXISTS idx_graph_signals_task_state;
DROP INDEX IF EXISTS idx_graph_work_item_edges_task_active;
DROP INDEX IF EXISTS idx_graph_work_item_edges_from;
DROP INDEX IF EXISTS idx_graph_work_item_edges_to;
DROP INDEX IF EXISTS idx_graph_work_items_profile;
DROP INDEX IF EXISTS idx_graph_work_items_task_active;

DROP TABLE IF EXISTS graph_signals;
DROP TABLE IF EXISTS graph_work_item_edges;
DROP TABLE IF EXISTS graph_work_items;
DROP TABLE IF EXISTS graph_tasks;
