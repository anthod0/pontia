import {
	error,
	redirect,
	type Cookies,
	type RequestEvent
} from '@sveltejs/kit';
import { database } from '../db';
import { activeLogin, bindAccount, login, logout } from './identity';
import { issueLogin, signedLogin, verifyLogin } from './jwt';
import { beginOAuth, exchangeAccount, OAUTH_SECONDS, readOAuth } from './oauth';
import { AuthError, type Provider } from './types';

const LOGIN_COOKIE = '_at';
const OAUTH_COOKIE = '_oauth';
const cookieOptions = {
	path: '/',
	httpOnly: true,
	secure: true,
	sameSite: 'lax' as const
};

export function environment(event: Pick<RequestEvent, 'platform'>) {
	if (!event.platform) error(503, 'Authentication is unavailable');
	return event.platform.env;
}

function provider(event: RequestEvent): Provider {
	if (event.params.provider !== 'google' && event.params.provider !== 'github')
		error(404, 'Unknown provider');
	return event.params.provider;
}

function origin(event: RequestEvent) {
	const configured = new URL(environment(event).AUTH_ORIGIN).origin;
	if (event.url.origin !== configured)
		error(400, 'Use the configured website address to sign in');
	return configured;
}

function sameOriginPost(event: RequestEvent) {
	if (event.request.headers.get('origin') !== origin(event))
		error(403, 'Invalid request origin');
}

export async function currentLogin(
	event: Pick<RequestEvent, 'platform' | 'cookies'>
) {
	const token = event.cookies.get(LOGIN_COOKIE);
	if (!token) return null;
	try {
		return await verifyLogin(token, environment(event).JWT_SECRET);
	} catch (cause) {
		if (cause instanceof AuthError) return null;
		throw cause;
	}
}

function clearCookies(cookies: Cookies) {
	cookies.delete(LOGIN_COOKIE, cookieOptions);
	cookies.delete(OAUTH_COOKIE, cookieOptions);
}

export async function startLogin(event: RequestEvent) {
	sameOriginPost(event);
	const selected = provider(event);
	const result = await beginOAuth(
		environment(event),
		selected,
		`${origin(event)}/api/auth/${selected}/callback`,
		{ kind: 'login' }
	);
	event.cookies.set(OAUTH_COOKIE, result.cookie, {
		...cookieOptions,
		maxAge: OAUTH_SECONDS
	});
	redirect(303, result.url);
}

export async function startBinding(event: RequestEvent) {
	sameOriginPost(event);
	const selected = provider(event);
	const claims = await currentLogin(event);
	const env = environment(event);
	if (
		!claims ||
		!(await activeLogin(database(env.DB), claims.sub, claims.user_id))
	)
		redirect(303, '/login?error=invalid_credentials');
	const result = await beginOAuth(
		env,
		selected,
		`${origin(event)}/api/auth/${selected}/callback`,
		{ kind: 'bind', loginId: claims.sub, userId: claims.user_id }
	);
	event.cookies.set(OAUTH_COOKIE, result.cookie, {
		...cookieOptions,
		maxAge: OAUTH_SECONDS
	});
	redirect(303, result.url);
}

export async function callback(event: RequestEvent) {
	const selected = provider(event);
	const env = environment(event);
	const callbackUri = `${origin(event)}/api/auth/${selected}/callback`;
	const cookie = event.cookies.get(OAUTH_COOKIE);
	event.cookies.delete(OAUTH_COOKIE, cookieOptions);
	let destination = '/login';
	try {
		if (!cookie) throw new AuthError('invalid_oauth');
		const oauth = await readOAuth(
			env,
			cookie,
			selected,
			event.url.searchParams.get('state') ?? '',
			callbackUri
		);
		destination = oauth.intent.kind === 'bind' ? '/account' : '/login';
		const code = event.url.searchParams.get('code');
		if (!code || event.url.searchParams.has('error'))
			throw new AuthError('invalid_oauth');
		const db = database(env.DB);
		if (oauth.intent.kind === 'bind') {
			const token = event.cookies.get(LOGIN_COOKIE);
			if (!token) throw new AuthError('invalid_credentials');
			const claims = await verifyLogin(token, env.JWT_SECRET);
			if (
				claims.sub !== oauth.intent.loginId ||
				claims.user_id !== oauth.intent.userId ||
				!(await activeLogin(db, claims.sub, claims.user_id))
			)
				throw new AuthError('invalid_credentials');
			const profile = await exchangeAccount(env, oauth, code);
			await bindAccount(db, profile, claims.sub, claims.user_id);
		} else {
			const profile = await exchangeAccount(env, oauth, code);
			const id = await login(db, profile);
			const credential = await issueLogin(db, id, env.JWT_SECRET);
			event.cookies.set(LOGIN_COOKIE, credential.token, {
				...cookieOptions,
				expires: credential.expiresAt
			});
		}
	} catch (cause) {
		if (!(cause instanceof AuthError)) throw cause;
		redirect(303, `${destination}?error=${cause.code}`);
	}
	redirect(303, '/account');
}

export async function endLogin(event: RequestEvent) {
	sameOriginPost(event);
	const token = event.cookies.get(LOGIN_COOKIE);
	try {
		if (token) {
			const env = environment(event);
			let claims;
			try {
				claims = await signedLogin(token, env.JWT_SECRET);
			} catch (cause) {
				if (!(cause instanceof AuthError)) throw cause;
			}
			if (claims) await logout(database(env.DB), claims.sub, claims.user_id);
		}
	} finally {
		clearCookies(event.cookies);
	}
	redirect(303, '/login');
}
