import { sql } from 'drizzle-orm';
import {
	check,
	index,
	integer,
	primaryKey,
	sqliteTable,
	text,
	uniqueIndex
} from 'drizzle-orm/sqlite-core';

const timestamp = sql`(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))`;

export const users = sqliteTable(
	'users',
	{
		id: text().notNull(),
		displayName: text('display_name'),
		avatarUrl: text('avatar_url'),
		createdAt: text('created_at').notNull().default(timestamp)
	},
	(table) => [primaryKey({ columns: [table.id] })]
);

export const accounts = sqliteTable(
	'accounts',
	{
		id: text().notNull(),
		userId: text('user_id')
			.notNull()
			.references(() => users.id, { onDelete: 'cascade' }),
		provider: text({ enum: ['google', 'github'] }).notNull(),
		providerSubject: text('provider_subject').notNull(),
		email: text(),
		emailVerified: integer('email_verified', { mode: 'boolean' })
			.notNull()
			.default(sql`0`),
		createdAt: text('created_at').notNull().default(timestamp),
		updatedAt: text('updated_at').notNull().default(timestamp)
	},
	(table) => [
		primaryKey({ columns: [table.id] }),
		check(
			'accounts_provider_check',
			sql`${table.provider} IN ('google', 'github')`
		),
		uniqueIndex('idx_accounts_provider_subject').on(
			table.provider,
			table.providerSubject
		),
		uniqueIndex('idx_accounts_user_provider').on(table.userId, table.provider)
	]
);

export const authSessions = sqliteTable(
	'auth_sessions',
	{
		id: text().notNull(),
		userId: text('user_id')
			.notNull()
			.references(() => users.id, { onDelete: 'cascade' }),
		accountId: text('account_id').references(() => accounts.id, {
			onDelete: 'set null'
		}),
		expiresAt: text('expires_at').notNull(),
		createdAt: text('created_at').notNull().default(timestamp)
	},
	(table) => [
		primaryKey({ columns: [table.id] }),
		index('idx_auth_sessions_user_id').on(table.userId)
	]
);
