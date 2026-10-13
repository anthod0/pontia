ALTER TABLE `auth_sessions` ADD `last_seen_at` text;--> statement-breakpoint
UPDATE `auth_sessions` SET `last_seen_at` = `created_at` WHERE `last_seen_at` IS NULL;