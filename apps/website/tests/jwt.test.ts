import { expect, test } from 'bun:test';
import { eq } from 'drizzle-orm';
import { CompactSign } from 'jose';
import { v7 as uuidv7 } from 'uuid';
import { authSessions, users } from '../src/lib/server/db/schema';
import { activeLogin, login, logout } from '../src/lib/server/auth/identity';
import {
	issueLogin,
	renewLogin,
	signingKey,
	verifyLogin
} from '../src/lib/server/auth/jwt';
import { testDatabase } from './database';

const database = testDatabase();
const secret = 'test-jwt-secret-with-at-least-32-bytes';
const now = new Date('2026-09-26T00:00:00Z');
const later = new Date('2026-09-26T01:00:00Z');

async function credential() {
	const id = await login(
		database.db,
		{
			provider: 'google',
			providerSubject: 'sub',
			email: null,
			emailVerified: false,
			displayName: 'Original',
			avatarUrl: null
		},
		now
	);
	const issued = await issueLogin(database.db, id, secret, now);
	return {
		id,
		token: issued.token,
		claims: await verifyLogin(issued.token, secret, now)
	};
}

test('JWT-only verification expires in one hour; renewal reads current profile without extending D1 expiry', async () => {
	const { id, token, claims } = await credential();
	expect(claims.exp - claims.iat).toBe(3600);
	await expect(verifyLogin(token, secret, later)).rejects.toThrow();
	await database.db
		.update(users)
		.set({ displayName: 'Updated', avatarUrl: 'https://example.com/new' })
		.where(eq(users.id, claims.user_id));
	const renewed = await renewLogin(database.db, token, secret, later);
	expect(await verifyLogin(renewed, secret, later)).toMatchObject({
		sub: id,
		user_id: claims.user_id,
		display_name: 'Updated',
		avatar_url: 'https://example.com/new',
		iat: claims.exp,
		exp: claims.exp + 3600
	});
	expect(
		(await activeLogin(database.db, id, claims.user_id, later))!.expiresAt
	).toBe('2026-10-26T00:00:00.000Z');
});

test('logout prevents renewal while already-issued JWTs remain valid until expiration', async () => {
	const { id, token, claims } = await credential();
	await logout(database.db, id, claims.user_id);
	expect(await verifyLogin(token, secret, now)).toMatchObject({ sub: id });
	await expect(renewLogin(database.db, token, secret, later)).rejects.toThrow();
});

test('renewal caps JWT lifetime at the fixed login expiry and refuses expiry or mismatched users', async () => {
	const { id, token, claims } = await credential();
	const nearEnd = new Date('2026-10-25T23:45:00Z');
	const renewed = await renewLogin(database.db, token, secret, nearEnd);
	expect((await verifyLogin(renewed, secret, nearEnd)).exp).toBe(
		Date.parse('2026-10-26T00:00:00Z') / 1000
	);
	await expect(
		renewLogin(database.db, token, secret, new Date('2026-10-26T00:00:00Z'))
	).rejects.toThrow();
	const otherId = uuidv7();
	await database.db.insert(users).values({ id: otherId });
	await database.db
		.update(authSessions)
		.set({ userId: otherId })
		.where(eq(authSessions.id, id));
	await expect(renewLogin(database.db, token, secret, later)).rejects.toThrow();
	expect(claims.user_id).not.toBe(otherId);
});

test('renewal rejects unexpired, tampered, wrong-algorithm, wrong-type and malformed signed credentials', async () => {
	const { token, claims } = await credential();
	await expect(renewLogin(database.db, token, secret, now)).rejects.toThrow();
	const parts = token.split('.');
	parts[1] = Buffer.from(
		JSON.stringify({ ...claims, user_id: uuidv7() })
	).toString('base64url');
	await expect(
		renewLogin(database.db, parts.join('.'), secret, later)
	).rejects.toThrow();
	for (const [payload, header] of [
		[claims, { alg: 'HS384', typ: 'at+jwt' }],
		[claims, { alg: 'HS256', typ: 'oauth-state+jwt' }],
		[
			{ ...claims, user_id: 'not-a-uuid' },
			{ alg: 'HS256', typ: 'at+jwt' }
		],
		[
			{ ...claims, exp: 'expired' },
			{ alg: 'HS256', typ: 'at+jwt' }
		],
		[
			{ ...claims, display_name: 123 },
			{ alg: 'HS256', typ: 'at+jwt' }
		],
		[
			{ ...claims, nbf: claims.exp + 1 },
			{ alg: 'HS256', typ: 'at+jwt' }
		]
	] as const) {
		const invalid = await new CompactSign(
			new TextEncoder().encode(JSON.stringify(payload))
		)
			.setProtectedHeader(header)
			.sign(signingKey(secret));
		await expect(
			renewLogin(database.db, invalid, secret, later)
		).rejects.toThrow();
	}
});
