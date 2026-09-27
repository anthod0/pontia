CREATE TABLE `device_authorizations` (
	`id` text PRIMARY KEY NOT NULL,
	`device_code_hash` text NOT NULL,
	`user_code` text NOT NULL,
	`status` text NOT NULL,
	`user_id` text,
	`expires_at` text NOT NULL,
	`last_polled_at` text,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_device_authorizations_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT "device_authorizations_status_check" CHECK("status" IN ('pending', 'approved', 'denied', 'consumed'))
);
--> statement-breakpoint
CREATE TABLE `device_rate_limits` (
	`key` text PRIMARY KEY NOT NULL,
	`window_started_at` text NOT NULL,
	`attempt_count` integer NOT NULL
);
--> statement-breakpoint
ALTER TABLE `auth_sessions` ADD `kind` text DEFAULT 'browser' NOT NULL;--> statement-breakpoint
ALTER TABLE `auth_sessions` ADD `token_hash` text;--> statement-breakpoint
PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_auth_sessions` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`account_id` text,
	`kind` text DEFAULT 'browser' NOT NULL,
	`token_hash` text,
	`expires_at` text,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_auth_sessions_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT `fk_auth_sessions_account_id_accounts_id_fk` FOREIGN KEY (`account_id`) REFERENCES `accounts`(`id`) ON DELETE SET NULL
);
--> statement-breakpoint
INSERT INTO `__new_auth_sessions`(`id`, `user_id`, `account_id`, `expires_at`, `created_at`) SELECT `id`, `user_id`, `account_id`, `expires_at`, `created_at` FROM `auth_sessions`;--> statement-breakpoint
DROP TABLE `auth_sessions`;--> statement-breakpoint
ALTER TABLE `__new_auth_sessions` RENAME TO `auth_sessions`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE INDEX `idx_auth_sessions_user_id` ON `auth_sessions` (`user_id`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_device_authorizations_device_code` ON `device_authorizations` (`device_code_hash`);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_device_authorizations_user_code` ON `device_authorizations` (`user_code`);--> statement-breakpoint
CREATE INDEX `idx_device_authorizations_expires_at` ON `device_authorizations` (`expires_at`);