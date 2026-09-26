import { compactVerify, SignJWT } from 'jose';
import type { Database } from '../db';
import { activeLogin } from './identity';
import { AuthError } from './types';

export interface LoginClaims {
	sub: string;
	iat: number;
	exp: number;
	user_id: string;
	display_name: string | null;
	avatar_url: string | null;
}

const uuidv7 =
	/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

export function signingKey(secret: string) {
	const key = new TextEncoder().encode(secret);
	if (key.length < 32)
		throw new Error(
			'Authentication signing keys must contain at least 32 bytes'
		);
	return key;
}

// Expiration is checked separately so only renewal and logout can accept expired credentials.
export async function signedLogin(
	token: string,
	secret: string,
	now = new Date()
): Promise<LoginClaims> {
	const key = signingKey(secret);
	try {
		const { payload, protectedHeader } = await compactVerify(token, key, {
			algorithms: ['HS256']
		});
		const claims = JSON.parse(
			new TextDecoder('utf-8', { fatal: true }).decode(payload)
		);
		const seconds = Math.floor(now.getTime() / 1000);
		if (
			protectedHeader.typ !== 'at+jwt' ||
			!claims ||
			typeof claims !== 'object' ||
			typeof claims.sub !== 'string' ||
			!uuidv7.test(claims.sub) ||
			typeof claims.user_id !== 'string' ||
			!uuidv7.test(claims.user_id) ||
			!Number.isSafeInteger(claims.iat) ||
			claims.iat < 0 ||
			claims.iat > seconds ||
			!Number.isSafeInteger(claims.exp) ||
			claims.exp <= claims.iat ||
			claims.exp > claims.iat + 3600 ||
			!(
				claims.display_name === null || typeof claims.display_name === 'string'
			) ||
			!(claims.avatar_url === null || typeof claims.avatar_url === 'string') ||
			('nbf' in claims &&
				(!Number.isSafeInteger(claims.nbf) || claims.nbf > seconds))
		) {
			throw new AuthError('invalid_credentials');
		}
		return claims;
	} catch {
		throw new AuthError('invalid_credentials');
	}
}

export async function verifyLogin(
	token: string,
	secret: string,
	now = new Date()
) {
	const claims = await signedLogin(token, secret, now);
	if (claims.exp <= Math.floor(now.getTime() / 1000))
		throw new AuthError('invalid_credentials');
	return claims;
}

export async function issueLogin(
	db: Database,
	id: string,
	secret: string,
	now = new Date(),
	userId?: string
) {
	const login = await activeLogin(db, id, userId, now);
	if (!login) throw new AuthError('invalid_credentials');
	const iat = Math.floor(now.getTime() / 1000);
	const exp = Math.min(
		iat + 3600,
		Math.floor(Date.parse(login.expiresAt) / 1000)
	);
	if (exp <= iat) throw new AuthError('invalid_credentials');
	const token = await new SignJWT({
		user_id: login.userId,
		display_name: login.displayName,
		avatar_url: login.avatarUrl
	})
		.setProtectedHeader({ alg: 'HS256', typ: 'at+jwt' })
		.setSubject(login.id)
		.setIssuedAt(iat)
		.setExpirationTime(exp)
		.sign(signingKey(secret));
	return { token, expiresAt: new Date(login.expiresAt) };
}

export async function renewLogin(
	db: Database,
	oldToken: string,
	secret: string,
	now = new Date()
) {
	const claims = await signedLogin(oldToken, secret, now);
	if (claims.exp > Math.floor(now.getTime() / 1000))
		throw new AuthError('invalid_credentials');
	return (await issueLogin(db, claims.sub, secret, now, claims.user_id)).token;
}
