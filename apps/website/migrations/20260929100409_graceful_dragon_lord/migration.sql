PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_edges` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`access_scope` text DEFAULT 'private' NOT NULL,
	`name` text NOT NULL,
	`tunnel_url` text NOT NULL,
	`service_credential_hash` text NOT NULL,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	`updated_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_edges_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE RESTRICT,
	CONSTRAINT "edges_access_scope_check" CHECK("access_scope" IN ('private', 'public'))
);
--> statement-breakpoint
DROP TABLE `edges`;--> statement-breakpoint
ALTER TABLE `__new_edges` RENAME TO `edges`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE INDEX `idx_edges_user_id` ON `edges` (`user_id`);--> statement-breakpoint
CREATE INDEX `idx_edges_access_scope` ON `edges` (`access_scope`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_edges_tunnel_url` ON `edges` (`tunnel_url`);