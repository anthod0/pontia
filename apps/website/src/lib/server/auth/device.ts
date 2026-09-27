import { and, eq, gt, isNotNull, isNull, lte, or, sql } from 'drizzle-orm';
import { base64url } from 'jose';
import { v7 as uuidv7 } from 'uuid';
import type { Database } from '../db';
import {
	authSessions,
	deviceAuthorizations,
	deviceRateLimits
} from '../db/schema';

export const DEVICE_AUTHORIZATION_SECONDS = 5 * 60;
export const DEVICE_POLL_INTERVAL_SECONDS = 5;
const USER_CODE_ALPHABET = 'BCDFGHJKLMNPQRSTVWXZ';
const RATE_LIMIT_WINDOW_SECONDS = 5 * 60;
const USER_CODE_ATTEMPT_LIMIT = 10;
const AUTHORIZATION_ATTEMPT_LIMIT = 20;
const POLL_ATTEMPT_LIMIT = 120;

export type DevicePollResult =
	| {
			status:
				| 'authorization_pending'
				| 'slow_down'
				| 'access_denied'
				| 'expired_token';
	  }
	| { status: 'authorized'; token: string };

function randomBytes(length: number) {
	return crypto.getRandomValues(new Uint8Array(length));
}

async function hash(value: string) {
	return base64url.encode(
		new Uint8Array(
			await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))
		)
	);
}

function createUserCode() {
	let code = '';
	while (code.length < 8) {
		for (const value of randomBytes(8)) {
			if (value >= 240) continue;
			code += USER_CODE_ALPHABET[value % USER_CODE_ALPHABET.length];
			if (code.length === 8) break;
		}
	}
	return code;
}

export function parseUserCode(value: string) {
	return /^[BCDFGHJKLMNPQRSTVWXZ]{4}-[BCDFGHJKLMNPQRSTVWXZ]{4}$/.test(
		value
	)
		? value.replace('-', '')
		: null;
}

export function displayUserCode(value: string) {
	return `${value.slice(0, 4)}-${value.slice(4)}`;
}

export async function beginDeviceAuthorization(
	db: Database,
	verificationUri: string,
	now = new Date()
) {
	for (let attempt = 0; attempt < 8; attempt++) {
		const deviceCode = base64url.encode(randomBytes(32));
		const userCode = createUserCode();
		try {
			await db.insert(deviceAuthorizations).values({
				id: uuidv7(),
				deviceCodeHash: await hash(deviceCode),
				userCode,
				status: 'pending',
				expiresAt: new Date(
					now.getTime() + DEVICE_AUTHORIZATION_SECONDS * 1000
				).toISOString(),
				createdAt: now.toISOString()
			});
			return {
				device_code: deviceCode,
				user_code: displayUserCode(userCode),
				verification_uri: verificationUri,
				expires_in: DEVICE_AUTHORIZATION_SECONDS,
				interval: DEVICE_POLL_INTERVAL_SECONDS
			};
		} catch (cause) {
			if (attempt === 7) throw cause;
		}
	}
	throw new Error('Unable to create a device authorization');
}

async function rateLimitKey(scope: string, subject: string) {
	return `${scope}:${await hash(subject)}`;
}

async function recordRateLimit(
	db: Database,
	key: string,
	attemptLimit: number,
	now = new Date()
) {
	const startedAt = now.toISOString();
	const cutoff = new Date(
		now.getTime() - RATE_LIMIT_WINDOW_SECONDS * 1000
	).toISOString();
	const [limit] = await db
		.insert(deviceRateLimits)
		.values({ key, windowStartedAt: startedAt, attemptCount: 1 })
		.onConflictDoUpdate({
			target: deviceRateLimits.key,
			set: {
				windowStartedAt: sql`CASE WHEN ${deviceRateLimits.windowStartedAt} <= ${cutoff} THEN ${startedAt} ELSE ${deviceRateLimits.windowStartedAt} END`,
				attemptCount: sql`CASE WHEN ${deviceRateLimits.windowStartedAt} <= ${cutoff} THEN 1 ELSE ${deviceRateLimits.attemptCount} + 1 END`
			}
		})
		.returning({ attemptCount: deviceRateLimits.attemptCount });
	return limit.attemptCount <= attemptLimit;
}

export async function recordUserCodeAttempt(
	db: Database,
	loginId: string,
	now = new Date()
) {
	return recordRateLimit(
		db,
		await rateLimitKey('user-code', loginId),
		USER_CODE_ATTEMPT_LIMIT,
		now
	);
}

export async function recordAuthorizationAttempt(
	db: Database,
	clientAddress: string,
	now = new Date()
) {
	return recordRateLimit(
		db,
		await rateLimitKey('authorize', clientAddress),
		AUTHORIZATION_ATTEMPT_LIMIT,
		now
	);
}

export async function recordPollAttempt(
	db: Database,
	clientAddress: string,
	now = new Date()
) {
	return recordRateLimit(
		db,
		await rateLimitKey('poll', clientAddress),
		POLL_ATTEMPT_LIMIT,
		now
	);
}

export async function decideDeviceAuthorization(
	db: Database,
	userCode: string,
	userId: string,
	decision: 'approved' | 'denied',
	now = new Date()
) {
	const normalized = parseUserCode(userCode);
	if (!normalized) return false;
	const updated = await db
		.update(deviceAuthorizations)
		.set({
			status: decision,
			userId: decision === 'approved' ? userId : null
		})
		.where(
			and(
				eq(deviceAuthorizations.userCode, normalized),
				eq(deviceAuthorizations.status, 'pending'),
				gt(deviceAuthorizations.expiresAt, now.toISOString())
			)
		)
		.returning({ id: deviceAuthorizations.id });
	return updated.length === 1;
}

export async function pollDeviceAuthorization(
	db: Database,
	deviceCode: string,
	now = new Date()
): Promise<DevicePollResult> {
	if (!/^[A-Za-z0-9_-]{43}$/.test(deviceCode))
		return { status: 'expired_token' };
	const deviceCodeHash = await hash(deviceCode);
	const existing = await db
		.select({
			status: deviceAuthorizations.status,
			expiresAt: deviceAuthorizations.expiresAt
		})
		.from(deviceAuthorizations)
		.where(eq(deviceAuthorizations.deviceCodeHash, deviceCodeHash))
		.get();
	if (!existing || existing.expiresAt <= now.toISOString())
		return { status: 'expired_token' };
	if (existing.status === 'denied') return { status: 'access_denied' };
	if (existing.status === 'consumed') return { status: 'expired_token' };

	const threshold = new Date(
		now.getTime() - DEVICE_POLL_INTERVAL_SECONDS * 1000
	).toISOString();
	const polled = await db
		.update(deviceAuthorizations)
		.set({ lastPolledAt: now.toISOString() })
		.where(
			and(
				eq(deviceAuthorizations.deviceCodeHash, deviceCodeHash),
				gt(deviceAuthorizations.expiresAt, now.toISOString()),
				or(
					isNull(deviceAuthorizations.lastPolledAt),
					lte(deviceAuthorizations.lastPolledAt, threshold)
				)
			)
		)
		.returning({ status: deviceAuthorizations.status });
	if (!polled.length) return { status: 'slow_down' };
	if (polled[0].status === 'pending')
		return { status: 'authorization_pending' };
	if (polled[0].status === 'denied') return { status: 'access_denied' };
	if (polled[0].status !== 'approved') return { status: 'expired_token' };

	const id = uuidv7();
	const secret = base64url.encode(randomBytes(32));
	const tokenHash = await hash(secret);
	const [inserted, consumed] = await db.batch([
		db
			.insert(authSessions)
			.select(
				db
					.select({
						id: sql<string>`${id}`.as('id'),
						userId: sql<string>`${deviceAuthorizations.userId}`.as('user_id'),
						accountId: sql<null>`NULL`.as('account_id'),
						kind: sql<'cli'>`'cli'`.as('kind'),
						tokenHash: sql<string>`${tokenHash}`.as('token_hash'),
						expiresAt: sql<null>`NULL`.as('expires_at'),
						createdAt: sql<string>`${now.toISOString()}`.as('created_at')
					})
					.from(deviceAuthorizations)
					.where(
						and(
							eq(deviceAuthorizations.deviceCodeHash, deviceCodeHash),
							eq(deviceAuthorizations.status, 'approved'),
							gt(deviceAuthorizations.expiresAt, now.toISOString()),
							isNotNull(deviceAuthorizations.userId)
						)
					)
			)
			.returning({ id: authSessions.id }),
		db
			.update(deviceAuthorizations)
			.set({ status: 'consumed' })
			.where(
				and(
					eq(deviceAuthorizations.deviceCodeHash, deviceCodeHash),
					eq(deviceAuthorizations.status, 'approved'),
					gt(deviceAuthorizations.expiresAt, now.toISOString())
				)
			)
			.returning({ id: deviceAuthorizations.id })
	]);
	if (inserted.length !== 1 || consumed.length !== 1)
		return { status: 'expired_token' };
	return { status: 'authorized', token: `ptr_v1_${id}_${secret}` };
}
