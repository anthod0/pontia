ALTER TABLE `devices` ADD `edge_mode` text DEFAULT 'auto' NOT NULL;
--> statement-breakpoint
UPDATE devices SET edge_mode = 'fixed';
