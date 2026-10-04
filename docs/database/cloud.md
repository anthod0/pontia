# Cloud database

```sql
CREATE TABLE `accounts` (
	`id` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`provider` text NOT NULL,
	`provider_subject` text NOT NULL,
	`email` text,
	`email_verified` integer DEFAULT 0 NOT NULL,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	`updated_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL,
	CONSTRAINT `fk_accounts_user_id_users_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON DELETE CASCADE,
	CONSTRAINT "accounts_provider_check" CHECK("provider" IN ('google', 'github'))
);

CREATE TABLE `auth_sessions` (
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

CREATE TABLE `edges` (
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
);

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

CREATE TABLE `device_rate_limits` (
	`key` text PRIMARY KEY NOT NULL,
	`window_started_at` text NOT NULL,
	`attempt_count` integer NOT NULL
);

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
	CONSTRAINT "edge_tickets_purpose_check" CHECK("purpose" IN ('edge_deployment', 'device_tunnel')),
	CONSTRAINT "edge_tickets_payload_json_check" CHECK(json_valid("payload"))
);

CREATE TABLE `hero_name_heroes` (
	`word` text PRIMARY KEY NOT NULL
);

CREATE TABLE `hero_name_modifiers` (
	`word` text PRIMARY KEY NOT NULL
);

CREATE TABLE `users` (
	`id` text PRIMARY KEY NOT NULL,
	`display_name` text,
	`avatar_url` text,
	`created_at` text DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) NOT NULL
);

CREATE UNIQUE INDEX `idx_accounts_provider_subject` ON `accounts` (`provider`,`provider_subject`);
CREATE UNIQUE INDEX `idx_accounts_user_provider` ON `accounts` (`user_id`,`provider`);
CREATE INDEX `idx_auth_sessions_user_id` ON `auth_sessions` (`user_id`);
CREATE UNIQUE INDEX `idx_auth_sessions_token_hash` ON `auth_sessions` (`token_hash`);
CREATE INDEX `idx_edges_user_id` ON `edges` (`user_id`);
CREATE INDEX `idx_edges_access_scope` ON `edges` (`access_scope`);
CREATE UNIQUE INDEX `idx_edges_tunnel_url` ON `edges` (`tunnel_url`);
CREATE INDEX `idx_devices_user_id` ON `devices` (`user_id`);
CREATE INDEX `idx_devices_edge_id` ON `devices` (`edge_id`);
CREATE UNIQUE INDEX `idx_devices_user_handle` ON `devices` (`user_id`,`handle`);
CREATE UNIQUE INDEX `idx_devices_e2e_public_key` ON `devices` (`e2e_public_key`);
CREATE INDEX `idx_devices_e2e_key_version` ON `devices` (`e2e_key_version`);
CREATE UNIQUE INDEX `idx_device_authorizations_device_code` ON `device_authorizations` (`device_code_hash`);
CREATE UNIQUE INDEX `idx_device_authorizations_user_code` ON `device_authorizations` (`user_code`);
CREATE INDEX `idx_device_authorizations_expires_at` ON `device_authorizations` (`expires_at`);
CREATE UNIQUE INDEX `idx_edge_tickets_secret_hash` ON `edge_tickets` (`secret_hash`);
CREATE INDEX `idx_edge_tickets_expires_at` ON `edge_tickets` (`expires_at`);
```
