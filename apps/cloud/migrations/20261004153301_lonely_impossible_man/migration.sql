ALTER TABLE `devices` ADD `e2e_public_key` text;--> statement-breakpoint
ALTER TABLE `devices` ADD `e2e_key_version` integer;--> statement-breakpoint
DELETE FROM `edge_tickets` WHERE `purpose` = 'dashboard_access';--> statement-breakpoint
PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_edge_tickets` (
	`id` integer PRIMARY KEY,
	`purpose` text NOT NULL,
	`secret_hash` text NOT NULL,
	`user_id` text NOT NULL,
	`expected_edge_id` text NOT NULL,
	`payload` text NOT NULL,
	`expires_at` text NOT NULL,
	`consumed_at` text,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_edge_tickets_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT "edge_tickets_purpose_check" CHECK("purpose" IN ('edge_deployment', 'device_tunnel')),
	CONSTRAINT "edge_tickets_payload_json_check" CHECK(json_valid("payload"))
);
--> statement-breakpoint
INSERT INTO `__new_edge_tickets`(`id`, `purpose`, `secret_hash`, `user_id`, `expected_edge_id`, `payload`, `expires_at`, `consumed_at`, `created_at`) SELECT `id`, `purpose`, `secret_hash`, `user_id`, `expected_edge_id`, `payload`, `expires_at`, `consumed_at`, `created_at` FROM `edge_tickets`;--> statement-breakpoint
DROP TABLE `edge_tickets`;--> statement-breakpoint
ALTER TABLE `__new_edge_tickets` RENAME TO `edge_tickets`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE UNIQUE INDEX `idx_edge_tickets_secret_hash` ON `edge_tickets` (`secret_hash`);--> statement-breakpoint
CREATE INDEX `idx_edge_tickets_expires_at` ON `edge_tickets` (`expires_at`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_devices_e2e_public_key` ON `devices` (`e2e_public_key`);--> statement-breakpoint
CREATE INDEX `idx_devices_e2e_key_version` ON `devices` (`e2e_key_version`);