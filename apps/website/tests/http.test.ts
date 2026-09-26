import { mockProvider } from './provider';
import { afterEach, expect, test } from 'bun:test';
import { isRedirect, type RequestEvent } from '@sveltejs/kit';
import {
	startLogin,
	startBinding,
	callback,
	endLogin
} from '../src/lib/server/auth/http';
import { activeLogin, login, logout } from '../src/lib/server/auth/identity';
import {
	issueLogin,
	renewLogin,
	verifyLogin
} from '../src/lib/server/auth/jwt';
import { accounts, users } from '../src/lib/server/db/schema';
import { testDatabase } from './database';

const database = testDatabase();
const config = {
	AUTH_ORIGIN: 'https://example.com',
	GOOGLE_CLIENT_ID: 'google',
	GOOGLE_CLIENT_SECRET: 'google-secret',
	GITHUB_CLIENT_ID: 'github',
	GITHUB_CLIENT_SECRET: 'github-secret',
	JWT_SECRET: 'test-login-signing-key-at-least-32-bytes',
	OAUTH_COOKIE_SECRET: 'test-oauth-signing-key-at-least-32-bytes'
};
let fetchSpy: ReturnType<typeof mockProvider> | undefined;
afterEach(() => fetchSpy?.mockRestore());

function browser() {
	const cookies = new Map<string, string>();
	const options = new Map<string, Record<string, unknown>>();
	return {
		cookies,
		options,
		event(path: string, method = 'POST', requestOrigin = config.AUTH_ORIGIN) {
			const url = new URL(path, config.AUTH_ORIGIN);
			return {
				url,
				params: { provider: url.pathname.split('/')[3] },
				request: new Request(url, {
					method,
					headers: { origin: requestOrigin }
				}),
				platform: { env: { ...config, DB: database.binding } },
				cookies: {
					get: (name: string) => cookies.get(name),
					set: (
						name: string,
						value: string,
						attributes: Record<string, unknown>
					) => {
						cookies.set(name, value);
						options.set(name, attributes);
					},
					delete: (name: string) => cookies.delete(name)
				}
			} as unknown as RequestEvent;
		}
	};
}

async function location(action: Promise<unknown>) {
	try {
		await action;
	} catch (cause) {
		if (isRedirect(cause)) return cause.location;
		throw cause;
	}
	throw new Error('Expected a redirect');
}

function providerResponses() {
	fetchSpy = mockProvider(async (url) => {
		if (String(url).includes('token'))
			return Response.json({ access_token: 'transient-provider-token' });
		if (String(url).endsWith('/emails')) return Response.json([]);
		return Response.json(
			String(url).includes('google')
				? { sub: 'google-subject', name: 'First' }
				: { id: 123, login: 'github-user' }
		);
	});
}

async function signedIn(client: ReturnType<typeof browser>) {
	const authUrl = new URL(
		await location(startLogin(client.event('/api/auth/google/login')))
	);
	expect(client.options.get('_oauth')).toMatchObject({
		httpOnly: true,
		secure: true,
		sameSite: 'lax',
		maxAge: 600
	});
	const path = `/api/auth/google/callback?code=code&state=${authUrl.searchParams.get('state')}`;
	expect(await location(callback(client.event(path, 'GET')))).toBe('/account');
	return await verifyLogin(client.cookies.get('_at')!, config.JWT_SECRET);
}

test('HTTP login, linking and logout persist identity and set and clear protected cookies', async () => {
	providerResponses();
	const client = browser();
	const claims = await signedIn(client);
	expect(client.options.get('_at')).toMatchObject({
		httpOnly: true,
		secure: true,
		sameSite: 'lax',
		path: '/'
	});
	expect(
		(client.options.get('_at')!.expires as Date).getTime() - Date.now()
	).toBeGreaterThan(29 * 86_400_000);
	expect(client.cookies.has('_oauth')).toBe(false);
	const bindingUrl = new URL(
		await location(startBinding(client.event('/api/auth/github/bind')))
	);
	expect(
		await location(
			callback(
				client.event(
					`/api/auth/github/callback?code=code&state=${bindingUrl.searchParams.get('state')}`,
					'GET'
				)
			)
		)
	).toBe('/account');
	expect(await database.db.select().from(accounts)).toHaveLength(2);
	expect(await database.db.select().from(users)).toHaveLength(1);
	const oldToken = client.cookies.get('_at')!;
	await location(startBinding(client.event('/api/auth/github/bind')));
	expect(await location(endLogin(client.event('/api/auth/logout')))).toBe(
		'/login'
	);
	expect(client.cookies.size).toBe(0);
	expect(await activeLogin(database.db, claims.sub)).toBeUndefined();
	expect(await verifyLogin(oldToken, config.JWT_SECRET)).toMatchObject({
		sub: claims.sub
	});
	await expect(
		renewLogin(
			database.db,
			oldToken,
			config.JWT_SECRET,
			new Date((claims.exp + 1) * 1000)
		)
	).rejects.toThrow();
});

test('HTTP endpoints reject cross-origin actions and tampered callback state before provider access', async () => {
	const client = browser();
	for (const [path, handler] of [
		['/api/auth/google/login', startLogin],
		['/api/auth/github/bind', startBinding],
		['/api/auth/logout', endLogin]
	] as const) {
		await expect(
			handler(client.event(path, 'POST', 'https://attacker.example'))
		).rejects.toMatchObject({ status: 403 });
	}
	await location(startLogin(client.event('/api/auth/google/login')));
	fetchSpy = mockProvider(async () => {
		throw new Error('Unexpected provider request');
	});
	expect(
		await location(
			callback(
				client.event('/api/auth/google/callback?code=code&state=wrong', 'GET')
			)
		)
	).toBe('/login?error=invalid_oauth');
	expect(fetchSpy).not.toHaveBeenCalled();
	expect(client.cookies.has('_oauth')).toBe(false);
});

test('binding callback rejects logout in another request and a browser switched to another user', async () => {
	providerResponses();
	for (const switchUser of [false, true]) {
		const client = browser();
		const claims = await signedIn(client);
		const bindingUrl = new URL(
			await location(startBinding(client.event('/api/auth/github/bind')))
		);
		if (switchUser) {
			const id = await login(database.db, {
				provider: 'google',
				providerSubject: 'other-user',
				email: null,
				emailVerified: false,
				displayName: null,
				avatarUrl: null
			});
			client.cookies.set(
				'_at',
				(await issueLogin(database.db, id, config.JWT_SECRET)).token
			);
		} else await logout(database.db, claims.sub, claims.user_id);
		expect(
			await location(
				callback(
					client.event(
						`/api/auth/github/callback?code=code&state=${bindingUrl.searchParams.get('state')}`,
						'GET'
					)
				)
			)
		).toBe('/account?error=invalid_credentials');
	}
	expect(
		(await database.db.select().from(accounts)).some(
			(account) => account.provider === 'github'
		)
	).toBe(false);
});

test('binding callback rejects a JWT that expired during the OAuth round trip', async () => {
	providerResponses();
	const client = browser();
	const claims = await signedIn(client);
	const bindingUrl = new URL(
		await location(startBinding(client.event('/api/auth/github/bind')))
	);
	const past = new Date(Date.now() - 2 * 3_600_000);
	client.cookies.set(
		'_at',
		(await issueLogin(database.db, claims.sub, config.JWT_SECRET, past)).token
	);
	expect(
		await location(
			callback(
				client.event(
					`/api/auth/github/callback?code=code&state=${bindingUrl.searchParams.get('state')}`,
					'GET'
				)
			)
		)
	).toBe('/account?error=invalid_credentials');
	expect(await database.db.select().from(accounts)).toHaveLength(1);
});

test('expired JWT can still log out and remove its D1 login record', async () => {
	const client = browser();
	const past = new Date(Date.now() - 2 * 3_600_000);
	const id = await login(
		database.db,
		{
			provider: 'github',
			providerSubject: '99',
			email: null,
			emailVerified: false,
			displayName: null,
			avatarUrl: null
		},
		past
	);
	client.cookies.set(
		'_at',
		(await issueLogin(database.db, id, config.JWT_SECRET, past)).token
	);
	expect(await location(endLogin(client.event('/api/auth/logout')))).toBe(
		'/login'
	);
	expect(await activeLogin(database.db, id)).toBeUndefined();
	expect(client.cookies.has('_at')).toBe(false);
});
