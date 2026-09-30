CREATE TABLE `edge_tickets` (
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
	CONSTRAINT "edge_tickets_purpose_check" CHECK("purpose" IN ('edge_deployment', 'device_tunnel', 'dashboard_access')),
	CONSTRAINT "edge_tickets_payload_json_check" CHECK(json_valid("payload"))
);
--> statement-breakpoint
DROP INDEX IF EXISTS `idx_tunnel_tickets_expires_at`;--> statement-breakpoint
CREATE UNIQUE INDEX `idx_edge_tickets_secret_hash` ON `edge_tickets` (`secret_hash`);--> statement-breakpoint
CREATE INDEX `idx_edge_tickets_expires_at` ON `edge_tickets` (`expires_at`);--> statement-breakpoint
DROP TABLE `tunnel_tickets`;