import { mockProvider } from './provider';
import { afterEach, expect, test } from 'bun:test';
import { base64url, decodeJwt } from 'jose';
import {
	beginOAuth,
	exchangeAccount,
	readOAuth
} from '../src/lib/server/auth/oauth';

const config = {
	GOOGLE_CLIENT_ID: 'google-client',
	GOOGLE_CLIENT_SECRET: 'google-secret',
	GITHUB_CLIENT_ID: 'github-client',
	GITHUB_CLIENT_SECRET: 'github-secret',
	OAUTH_COOKIE_SECRET: 'oauth-test-secret-with-at-least-32-bytes'
};
const now = new Date('2026-09-26T00:00:00Z');
afterEach(() => {
	fetchSpy?.mockRestore();
});
let fetchSpy: ReturnType<typeof mockProvider> | undefined;

test('both providers use S256 PKCE, state, and the matching verifier in token exchange', async () => {
	for (const provider of ['google', 'github'] as const) {
		const callback = `https://example.com/api/auth/${provider}/callback`;
		const started = await beginOAuth(
			config,
			provider,
			callback,
			{ kind: 'login' },
			now
		);
		const url = new URL(started.url);
		const oauth = await readOAuth(
			config,
			started.cookie,
			provider,
			url.searchParams.get('state')!,
			callback,
			now
		);
		const challenge = base64url.encode(
			new Uint8Array(
				await crypto.subtle.digest(
					'SHA-256',
					new TextEncoder().encode(oauth.verifier)
				)
			)
		);
		expect(url.searchParams.get('code_challenge_method')).toBe('S256');
		expect(url.searchParams.get('code_challenge')).toBe(challenge);
		fetchSpy = mockProvider(async (input, init) => {
			if (String(input).includes('token')) {
				const body = init?.body as URLSearchParams;
				expect(body.get('code_verifier')).toBe(oauth.verifier);
				expect(body.get('redirect_uri')).toBe(callback);
				expect(body.get('code')).toBe('provider-code');
				return Response.json({ access_token: 'provider-token' });
			}
			if (String(input).endsWith('/emails'))
				return Response.json([
					{ email: 'unverified@example.com', primary: true, verified: false }
				]);
			return Response.json(
				provider === 'google'
					? { sub: 'stable-google-id', name: 'Name' }
					: { id: 42, login: 'changing-handle' }
			);
		});
		const account = await exchangeAccount(config, oauth, 'provider-code');
		expect(account.providerSubject).toBe(
			provider === 'google' ? 'stable-google-id' : '42'
		);
		expect(account.email).toBe(
			provider === 'google' ? null : 'unverified@example.com'
		);
		expect(account.emailVerified).toBe(false);
		fetchSpy.mockRestore();
	}
});

test('OAuth cookies reject state, provider, callback, expiry and signature mismatches', async () => {
	const callback = 'https://example.com/api/auth/google/callback';
	const started = await beginOAuth(
		config,
		'google',
		callback,
		{ kind: 'bind', loginId: 'login-id', userId: 'user-id' },
		now
	);
	const state = new URL(started.url).searchParams.get('state')!;
	expect(
		(await readOAuth(config, started.cookie, 'google', state, callback, now))
			.intent
	).toEqual({ kind: 'bind', loginId: 'login-id', userId: 'user-id' });
	await expect(
		readOAuth(config, started.cookie, 'google', 'wrong', callback, now)
	).rejects.toThrow('invalid_oauth');
	await expect(
		readOAuth(config, started.cookie, 'github', state, callback, now)
	).rejects.toThrow('invalid_oauth');
	await expect(
		readOAuth(config, started.cookie, 'google', state, callback + '/wrong', now)
	).rejects.toThrow('invalid_oauth');
	await expect(
		readOAuth(
			config,
			started.cookie,
			'google',
			state,
			callback,
			new Date(now.getTime() + 600_000)
		)
	).rejects.toThrow('invalid_oauth');
	const parts = started.cookie.split('.');
	parts[1] = base64url.encode(
		JSON.stringify({ ...decodeJwt(started.cookie), intent: { kind: 'login' } })
	);
	await expect(
		readOAuth(config, parts.join('.'), 'google', state, callback, now)
	).rejects.toThrow('invalid_oauth');
});

test('provider failures and missing subjects cannot establish identity', async () => {
	const callback = 'https://example.com/api/auth/google/callback';
	const started = await beginOAuth(
		config,
		'google',
		callback,
		{ kind: 'login' },
		now
	);
	const oauth = await readOAuth(
		config,
		started.cookie,
		'google',
		new URL(started.url).searchParams.get('state')!,
		callback,
		now
	);
	fetchSpy = mockProvider(async () =>
		Response.json({ error: 'invalid_grant' }, { status: 400 })
	);
	await expect(exchangeAccount(config, oauth, 'code')).rejects.toThrow(
		'provider_failed'
	);
	fetchSpy.mockImplementation(async (url) =>
		Response.json(
			String(url).includes('token')
				? { access_token: 'token' }
				: { email: 'email-is-not-identity@example.com' }
		)
	);
	await expect(exchangeAccount(config, oauth, 'code')).rejects.toThrow(
		'provider_failed'
	);
});
