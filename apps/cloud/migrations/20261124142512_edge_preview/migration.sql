ALTER TABLE `edges` ADD `preview_enabled` integer DEFAULT 0 NOT NULL CONSTRAINT "edges_preview_enabled_check" CHECK("preview_enabled" IN (0, 1));
