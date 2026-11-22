DROP TABLE bot_known_workspaces;
DELETE FROM bot_notifications WHERE id LIKE 'workspace:%' OR id LIKE 'todo:%';
