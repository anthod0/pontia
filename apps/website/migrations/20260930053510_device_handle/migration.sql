ALTER TABLE `devices` ADD `handle` text NOT NULL;--> statement-breakpoint
PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_devices` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`edge_id` text NOT NULL,
	`handle` text NOT NULL,
	`name` text,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	`updated_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_devices_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT `fk_devices_edge_id_edges_id_fk` FOREIGN KEY (`edge_id`) REFERENCES `edges`(`id`) ON DELETE RESTRICT,
	CONSTRAINT "devices_handle_format_check" CHECK(length("handle") BETWEEN 4 AND 48 AND substr("handle", 1, 1) GLOB '[a-z]' AND "handle" NOT GLOB '*[^a-z0-9_-]*')
);
--> statement-breakpoint
INSERT INTO `__new_devices`(`id`, `user_id`, `edge_id`, `handle`, `name`, `created_at`, `updated_at`) SELECT `id`, `user_id`, `edge_id`, `handle`, `name`, `created_at`, `updated_at` FROM `devices`;--> statement-breakpoint
DROP TABLE `devices`;--> statement-breakpoint
ALTER TABLE `__new_devices` RENAME TO `devices`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE INDEX `idx_devices_user_id` ON `devices` (`user_id`);--> statement-breakpoint
CREATE INDEX `idx_devices_edge_id` ON `devices` (`edge_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_devices_user_handle` ON `devices` (`user_id`,`handle`);