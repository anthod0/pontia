import { expect, test } from 'bun:test';
import {
	issuePendingAccount,
	readPendingAccount
} from '../src/lib/server/auth/pending';
import type { AccountProfile } from '../src/lib/server/auth/types';

const secret = 'pending-test-secret-with-at-least-32-bytes';
const now = new Date('2026-09-26T00:00:00Z');
const profile: AccountProfile = {
	provider: 'github',
	providerSubject: '123',
	email: 'same@example.com',
	emailVerified: true,
	displayName: 'Name',
	avatarUrl: null
};

test('pending account state is encrypted, authenticated, and expires after five minutes', async () => {
	const pending = await issuePendingAccount(
		secret,
		profile,
		'target-account',
		'/device?user_code=BCDF-GHJK',
		now
	);
	expect(pending.token.split('.')).toHaveLength(5);
	expect(pending.token).not.toContain('same@example.com');
	expect(await readPendingAccount(secret, pending.token, now)).toMatchObject({
		profile,
		targetAccountId: 'target-account',
		returnTo: '/device?user_code=BCDF-GHJK',
		jti: pending.jti
	});
	await expect(
		readPendingAccount(secret, pending.token, new Date(now.getTime() + 300_000))
	).rejects.toThrow('invalid_oauth');
	const parts = pending.token.split('.');
	parts[3] = `${parts[3].slice(0, -1)}${parts[3].endsWith('a') ? 'b' : 'a'}`;
	await expect(
		readPendingAccount(secret, parts.join('.'), now)
	).rejects.toThrow('invalid_oauth');
});
