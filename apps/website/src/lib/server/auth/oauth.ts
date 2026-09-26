import { base64url, jwtVerify, SignJWT } from 'jose';
import { signingKey } from './jwt';
import { AuthError, type AccountProfile, type Provider } from './types';

export interface OAuthConfig {
	GOOGLE_CLIENT_ID: string;
	GOOGLE_CLIENT_SECRET: string;
	GITHUB_CLIENT_ID: string;
	GITHUB_CLIENT_SECRET: string;
	OAUTH_COOKIE_SECRET: string;
}

type Intent =
	{ kind: 'login' } | { kind: 'bind'; loginId: string; userId: string };
interface OAuthState {
	provider: Provider;
	state: string;
	verifier: string;
	redirectUri: string;
	intent: Intent;
}

export const OAUTH_SECONDS = 600;
const endpoints = {
	google: {
		authorize: 'https://accounts.google.com/o/oauth2/v2/auth',
		token: 'https://oauth2.googleapis.com/token'
	},
	github: {
		authorize: 'https://github.com/login/oauth/authorize',
		token: 'https://github.com/login/oauth/access_token'
	}
};

function client(config: OAuthConfig, provider: Provider) {
	const id =
		provider === 'google' ? config.GOOGLE_CLIENT_ID : config.GITHUB_CLIENT_ID;
	const secret =
		provider === 'google'
			? config.GOOGLE_CLIENT_SECRET
			: config.GITHUB_CLIENT_SECRET;
	if (!id || !secret) throw new Error(`Missing ${provider} OAuth credentials`);
	return { id, secret };
}

export async function beginOAuth(
	config: OAuthConfig,
	provider: Provider,
	redirectUri: string,
	intent: Intent,
	now = new Date()
) {
	const { id } = client(config, provider);
	const state = base64url.encode(crypto.getRandomValues(new Uint8Array(32)));
	const verifier = base64url.encode(crypto.getRandomValues(new Uint8Array(32)));
	const challenge = base64url.encode(
		new Uint8Array(
			await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))
		)
	);
	const url = new URL(endpoints[provider].authorize);
	url.search = new URLSearchParams({
		client_id: id,
		redirect_uri: redirectUri,
		response_type: 'code',
		state,
		code_challenge: challenge,
		code_challenge_method: 'S256',
		scope:
			provider === 'google' ? 'openid profile email' : 'read:user user:email'
	}).toString();
	const cookie = await new SignJWT({
		provider,
		state,
		verifier,
		redirectUri,
		intent
	})
		.setProtectedHeader({ alg: 'HS256', typ: 'oauth-state+jwt' })
		.setIssuedAt(Math.floor(now.getTime() / 1000))
		.setExpirationTime(Math.floor(now.getTime() / 1000) + OAUTH_SECONDS)
		.sign(signingKey(config.OAUTH_COOKIE_SECRET));
	return { url: url.toString(), cookie };
}

export async function readOAuth(
	config: OAuthConfig,
	cookie: string,
	provider: Provider,
	state: string,
	redirectUri: string,
	now = new Date()
): Promise<OAuthState> {
	const key = signingKey(config.OAUTH_COOKIE_SECRET);
	try {
		const { payload } = await jwtVerify(cookie, key, {
			algorithms: ['HS256'],
			typ: 'oauth-state+jwt',
			currentDate: now,
			maxTokenAge: OAUTH_SECONDS,
			requiredClaims: ['exp', 'iat']
		});
		const intent = payload.intent as Intent | undefined;
		if (
			payload.provider !== provider ||
			payload.state !== state ||
			!state ||
			payload.redirectUri !== redirectUri ||
			typeof payload.verifier !== 'string' ||
			!/^[A-Za-z0-9_-]{43}$/.test(payload.verifier) ||
			!intent ||
			(intent.kind !== 'login' && intent.kind !== 'bind') ||
			(intent.kind === 'bind' &&
				(typeof intent.loginId !== 'string' ||
					typeof intent.userId !== 'string'))
		) {
			throw new AuthError('invalid_oauth');
		}
		return { provider, state, verifier: payload.verifier, redirectUri, intent };
	} catch {
		throw new AuthError('invalid_oauth');
	}
}

async function jsonRequest(url: string, init: RequestInit) {
	const response = await fetch(url, {
		...init,
		signal: AbortSignal.timeout(15_000)
	});
	if (!response.ok) throw new AuthError('provider_failed');
	return (await response.json()) as Record<string, unknown>;
}

const optionalText = (value: unknown) =>
	typeof value === 'string' && value ? value : null;

export async function exchangeAccount(
	config: OAuthConfig,
	oauth: OAuthState,
	code: string
): Promise<AccountProfile> {
	const { id, secret } = client(config, oauth.provider);
	try {
		const token = await jsonRequest(endpoints[oauth.provider].token, {
			method: 'POST',
			headers: { Accept: 'application/json' },
			body: new URLSearchParams({
				grant_type: 'authorization_code',
				client_id: id,
				client_secret: secret,
				code,
				redirect_uri: oauth.redirectUri,
				code_verifier: oauth.verifier
			})
		});
		if (
			typeof token.access_token !== 'string' ||
			!token.access_token ||
			token.error
		)
			throw new AuthError('provider_failed');
		const headers = {
			Authorization: `Bearer ${token.access_token}`,
			Accept: 'application/json',
			'User-Agent': 'Pontia'
		};
		if (oauth.provider === 'google') {
			const user = await jsonRequest(
				'https://openidconnect.googleapis.com/v1/userinfo',
				{ headers }
			);
			if (typeof user.sub !== 'string' || !user.sub)
				throw new AuthError('provider_failed');
			return {
				provider: 'google',
				providerSubject: user.sub,
				displayName: optionalText(user.name),
				avatarUrl: optionalText(user.picture),
				email: optionalText(user.email),
				emailVerified: user.email_verified === true
			};
		}
		const user = await jsonRequest('https://api.github.com/user', { headers });
		if (
			typeof user.id !== 'number' ||
			!Number.isSafeInteger(user.id) ||
			user.id <= 0
		)
			throw new AuthError('provider_failed');
		const emails = await jsonRequest('https://api.github.com/user/emails', {
			headers
		});
		if (!Array.isArray(emails)) throw new AuthError('provider_failed');
		const primary = emails.find(
			(email: Record<string, unknown>) => email.primary === true
		);
		return {
			provider: 'github',
			providerSubject: String(user.id),
			displayName: optionalText(user.name) ?? optionalText(user.login),
			avatarUrl: optionalText(user.avatar_url),
			email: optionalText(primary?.email),
			emailVerified: primary?.verified === true
		};
	} catch {
		throw new AuthError('provider_failed');
	}
}
