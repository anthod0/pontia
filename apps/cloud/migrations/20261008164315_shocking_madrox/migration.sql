CREATE TABLE `__devices_backup` AS SELECT * FROM `devices`;--> statement-breakpoint
DROP TABLE `devices`;--> statement-breakpoint
CREATE TABLE `__new_edges` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`access_scope` text DEFAULT 'private' NOT NULL,
	`name` text NOT NULL,
	`dns_label` text NOT NULL,
	`tunnel_url` text NOT NULL,
	`service_credential_hash` text NOT NULL,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	`updated_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_edges_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE RESTRICT,
	CONSTRAINT "edges_access_scope_check" CHECK("access_scope" IN ('private', 'public'))
);--> statement-breakpoint
INSERT INTO `__new_edges` (`id`, `user_id`, `access_scope`, `name`, `dns_label`, `tunnel_url`, `service_credential_hash`, `created_at`, `updated_at`)
SELECT `id`, `user_id`, `access_scope`, `name`, `name`, `tunnel_url`, `service_credential_hash`, `created_at`, `updated_at` FROM `edges`;--> statement-breakpoint
DROP TABLE `edges`;--> statement-breakpoint
ALTER TABLE `__new_edges` RENAME TO `edges`;--> statement-breakpoint
CREATE TABLE `devices` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`edge_id` text NOT NULL,
	`handle` text NOT NULL,
	`name` text NOT NULL,
	`e2e_public_key` text,
	`e2e_key_version` integer,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	`updated_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_devices_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT `fk_devices_edge_id_edges_id_fk` FOREIGN KEY (`edge_id`) REFERENCES `edges`(`id`) ON DELETE RESTRICT,
	CONSTRAINT "devices_handle_format_check" CHECK(length("handle") BETWEEN 4 AND 48 AND substr("handle", 1, 1) GLOB '[a-z]' AND "handle" NOT GLOB '*[^a-z0-9_-]*')
);--> statement-breakpoint
INSERT INTO `devices` (`id`, `user_id`, `edge_id`, `handle`, `name`, `e2e_public_key`, `e2e_key_version`, `created_at`, `updated_at`)
SELECT `id`, `user_id`, `edge_id`, `handle`, `name`, `e2e_public_key`, `e2e_key_version`, `created_at`, `updated_at` FROM `__devices_backup`;--> statement-breakpoint
DROP TABLE `__devices_backup`;--> statement-breakpoint
CREATE INDEX `idx_edges_user_id` ON `edges` (`user_id`);--> statement-breakpoint
CREATE INDEX `idx_edges_access_scope` ON `edges` (`access_scope`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_edges_dns_label` ON `edges` (`dns_label`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_edges_tunnel_url` ON `edges` (`tunnel_url`);--> statement-breakpoint
CREATE INDEX `idx_devices_user_id` ON `devices` (`user_id`);--> statement-breakpoint
CREATE INDEX `idx_devices_edge_id` ON `devices` (`edge_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_devices_user_handle` ON `devices` (`user_id`,`handle`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_devices_e2e_public_key` ON `devices` (`e2e_public_key`);--> statement-breakpoint
CREATE INDEX `idx_devices_e2e_key_version` ON `devices` (`e2e_key_version`);
