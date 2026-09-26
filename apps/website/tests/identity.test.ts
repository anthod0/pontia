import { expect, test } from 'bun:test';
import { eq } from 'drizzle-orm';
import { v7 as uuidv7 } from 'uuid';
import { accounts, authSessions, users } from '../src/lib/server/db/schema';
import {
	activeLogin,
	bindAccount,
	login,
	logout
} from '../src/lib/server/auth/identity';
import { type AccountProfile } from '../src/lib/server/auth/types';
import { testDatabase } from './database';

const database = testDatabase();
const now = new Date('2026-09-26T00:00:00.000Z');
export const profile: AccountProfile = {
	provider: 'google',
	providerSubject: 'google-subject',
	email: 'same@example.com',
	emailVerified: false,
	displayName: 'First name',
	avatarUrl: 'https://example.com/avatar.png'
};
const github: AccountProfile = {
	...profile,
	provider: 'github',
	providerSubject: '123'
};

test('first and returning login use subject, retain user profile, and create independent login periods', async () => {
	const { db } = database;
	const firstId = await login(db, profile, now);
	const first = (await activeLogin(db, firstId, undefined, now))!;
	expect(first.id).toMatch(
		/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
	);
	expect(first.expiresAt).toBe('2026-10-26T00:00:00.000Z');
	const second = await login(
		db,
		{
			...profile,
			email: 'changed@example.com',
			emailVerified: true,
			displayName: 'Changed',
			avatarUrl: null
		},
		now
	);
	expect(second).not.toBe(firstId);
	expect(await activeLogin(db, second, first.userId, now)).toMatchObject({
		displayName: 'First name',
		avatarUrl: profile.avatarUrl
	});
	expect(await db.select().from(accounts)).toMatchObject([
		{ email: 'changed@example.com', emailVerified: true }
	]);
	expect(await db.select().from(users)).toHaveLength(1);
	await logout(db, firstId, first.userId);
	expect(await activeLogin(db, firstId, first.userId, now)).toBeUndefined();
	expect(await activeLogin(db, second, first.userId, now)).toBeDefined();
});

test('equal emails, missing emails, and different subjects never merge users', async () => {
	const { db } = database;
	await login(db, profile, now);
	await login(db, github, now);
	await login(db, { ...profile, providerSubject: 'another', email: null }, now);
	await login(db, { ...profile, providerSubject: 'third', email: null }, now);
	expect(await db.select().from(users)).toHaveLength(4);
});

test('concurrent first logins create one user and Account without orphan users', async () => {
	const ids = await Promise.all([
		login(database.db, profile, now),
		login(database.db, profile, now)
	]);
	expect(new Set(ids).size).toBe(2);
	expect(await database.db.select().from(users)).toHaveLength(1);
	expect(await database.db.select().from(accounts)).toHaveLength(1);
});

test('binding preserves profile, permits login through the new provider, and rejects provider replacement', async () => {
	const { db } = database;
	const id = await login(db, profile, now);
	const original = (await activeLogin(db, id, undefined, now))!;
	await bindAccount(
		db,
		{ ...github, displayName: 'Other name' },
		id,
		original.userId,
		now
	);
	await expect(
		bindAccount(db, github, id, original.userId, now)
	).rejects.toThrow('account_conflict');
	await expect(
		bindAccount(
			db,
			{ ...github, providerSubject: '456' },
			id,
			original.userId,
			now
		)
	).rejects.toThrow('account_conflict');
	const linkedLogin = await login(db, github, now);
	expect(
		await activeLogin(db, linkedLogin, original.userId, now)
	).toMatchObject({ displayName: profile.displayName });
	expect(await db.select().from(users)).toHaveLength(1);
});

test('binding rejects accounts owned by others and revoked, expired, or mismatched login periods', async () => {
	const { db } = database;
	const id = await login(db, profile, now);
	const original = (await activeLogin(db, id, undefined, now))!;
	await login(db, github, now);
	await expect(
		bindAccount(db, github, id, original.userId, now)
	).rejects.toThrow('account_conflict');
	const other = { ...github, providerSubject: 'unowned' };
	await expect(bindAccount(db, other, id, uuidv7(), now)).rejects.toThrow(
		'invalid_credentials'
	);
	await expect(
		bindAccount(db, other, id, original.userId, new Date(original.expiresAt))
	).rejects.toThrow('invalid_credentials');
	await logout(db, id, original.userId);
	await expect(
		bindAccount(db, other, id, original.userId, now)
	).rejects.toThrow('invalid_credentials');
	expect(await db.select().from(accounts)).toHaveLength(2);
});

test('migration enforces null IDs, provider validity, atomic rollback, and deletion relationships', async () => {
	const { db, binding } = database;
	await expect(
		binding.prepare('INSERT INTO users(id) VALUES (NULL)').run()
	).rejects.toThrow();
	const userId = uuidv7();
	await expect(
		db.batch([
			db.insert(users).values({ id: userId }),
			db.insert(accounts).values({
				id: uuidv7(),
				userId,
				provider: 'unsupported' as 'google',
				providerSubject: 'x'
			})
		])
	).rejects.toThrow();
	expect(await db.select().from(users)).toHaveLength(0);
	const id = await login(db, profile, now);
	const original = (await activeLogin(db, id, undefined, now))!;
	await db.delete(accounts).where(eq(accounts.userId, original.userId));
	expect(await db.select().from(authSessions)).toMatchObject([
		{ accountId: null }
	]);
	expect(await activeLogin(db, id, original.userId, now)).toBeDefined();
	await db.delete(users).where(eq(users.id, original.userId));
	expect(await db.select().from(authSessions)).toHaveLength(0);
});
