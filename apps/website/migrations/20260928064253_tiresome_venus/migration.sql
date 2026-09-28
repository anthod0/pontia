CREATE TABLE `tunnel_tickets` (
	`id` text PRIMARY KEY NOT NULL,
	`secret_hash` text NOT NULL,
	`user_id` text NOT NULL,
	`device_id` text NOT NULL,
	`edge_id` text NOT NULL,
	`expires_at` text NOT NULL,
	`consumed_at` text,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_tunnel_tickets_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT `fk_tunnel_tickets_device_id_devices_id_fk` FOREIGN KEY (`device_id`) REFERENCES `devices`(`id`) ON DELETE CASCADE,
	CONSTRAINT `fk_tunnel_tickets_edge_id_edges_id_fk` FOREIGN KEY (`edge_id`) REFERENCES `edges`(`id`) ON DELETE CASCADE
);
--> statement-breakpoint
CREATE INDEX `idx_tunnel_tickets_expires_at` ON `tunnel_tickets` (`expires_at`);