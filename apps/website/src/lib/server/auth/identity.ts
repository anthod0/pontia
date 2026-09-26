import { and, eq, gt, sql } from 'drizzle-orm';
import { v7 as uuidv7 } from 'uuid';
import type { Database } from '../db';
import { accounts, authSessions, users } from '../db/schema';
import { AuthError, type AccountProfile } from './types';

export const AUTH_SESSION_SECONDS = 30 * 24 * 60 * 60;

export async function login(
	db: Database,
	profile: AccountProfile,
	now = new Date()
) {
	const subject = and(
		eq(accounts.provider, profile.provider),
		eq(accounts.providerSubject, profile.providerSubject)
	);
	let account = await db.select().from(accounts).where(subject).get();
	if (!account) {
		const userId = uuidv7();
		const accountId = uuidv7();
		try {
			await db.batch([
				db.insert(users).values({
					id: userId,
					displayName: profile.displayName,
					avatarUrl: profile.avatarUrl
				}),
				db.insert(accounts).values({
					id: accountId,
					userId,
					provider: profile.provider,
					providerSubject: profile.providerSubject,
					email: profile.email,
					emailVerified: profile.emailVerified
				})
			]);
		} catch (cause) {
			// A concurrent first login can win the unique subject; the failed batch rolls back its user.
			if (
				!(await db
					.select({ id: accounts.id })
					.from(accounts)
					.where(subject)
					.get())
			)
				throw cause;
		}
		account = await db.select().from(accounts).where(subject).get();
	}
	if (!account) throw new AuthError('invalid_credentials');
	const id = uuidv7();
	await db.batch([
		db
			.update(accounts)
			.set({
				email: profile.email,
				emailVerified: profile.emailVerified,
				updatedAt: now.toISOString()
			})
			.where(eq(accounts.id, account.id)),
		db.insert(authSessions).values({
			id,
			userId: account.userId,
			accountId: account.id,
			createdAt: now.toISOString(),
			expiresAt: new Date(
				now.getTime() + AUTH_SESSION_SECONDS * 1000
			).toISOString()
		})
	]);
	return id;
}

export async function activeLogin(
	db: Database,
	id: string,
	userId?: string,
	now = new Date()
) {
	return db
		.select({
			id: authSessions.id,
			userId: authSessions.userId,
			expiresAt: authSessions.expiresAt,
			displayName: users.displayName,
			avatarUrl: users.avatarUrl
		})
		.from(authSessions)
		.innerJoin(users, eq(users.id, authSessions.userId))
		.where(
			and(
				eq(authSessions.id, id),
				gt(authSessions.expiresAt, now.toISOString()),
				userId === undefined ? undefined : eq(authSessions.userId, userId)
			)
		)
		.get();
}

export async function bindAccount(
	db: Database,
	profile: AccountProfile,
	loginId: string,
	userId: string,
	now = new Date()
) {
	// Check revocation in the INSERT itself, so logout cannot race a separate validation query.
	const inserted = await db
		.insert(accounts)
		.select(
			db
				.select({
					id: sql<string>`${uuidv7()}`.as('id'),
					userId: authSessions.userId,
					provider: sql<AccountProfile['provider']>`${profile.provider}`.as(
						'provider'
					),
					providerSubject: sql<string>`${profile.providerSubject}`.as(
						'provider_subject'
					),
					email: sql<string | null>`${profile.email}`.as('email'),
					emailVerified: sql<boolean>`${profile.emailVerified ? 1 : 0}`.as(
						'email_verified'
					),
					createdAt: sql<string>`${now.toISOString()}`.as('created_at'),
					updatedAt: sql<string>`${now.toISOString()}`.as('updated_at')
				})
				.from(authSessions)
				.where(
					and(
						eq(authSessions.id, loginId),
						eq(authSessions.userId, userId),
						gt(authSessions.expiresAt, now.toISOString())
					)
				)
		)
		.onConflictDoNothing()
		.returning({ id: accounts.id });
	if (!inserted.length) {
		if (!(await activeLogin(db, loginId, userId, now)))
			throw new AuthError('invalid_credentials');
		throw new AuthError('account_conflict');
	}
}

export async function logout(db: Database, id: string, userId: string) {
	await db
		.delete(authSessions)
		.where(and(eq(authSessions.id, id), eq(authSessions.userId, userId)));
}
