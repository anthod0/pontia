import { json } from '@sveltejs/kit';
import {
	beginDeviceAuthorization,
	recordAuthorizationAttempt
} from '$lib/server/auth/device';
import { environment, origin } from '$lib/server/auth/http';
import { database } from '$lib/server/db';
import type { RequestHandler } from './$types';

export const POST: RequestHandler = async (event) => {
	const env = environment(event);
	const verificationUri = `${origin(event)}/device`;
	const db = database(env.DB);
	if (!(await recordAuthorizationAttempt(db, event.getClientAddress())))
		return json(
			{ error: 'slow_down' },
			{ status: 429, headers: { 'cache-control': 'private, no-store' } }
		);
	const authorization = await beginDeviceAuthorization(db, verificationUri);
	return json(authorization, {
		status: 201,
		headers: { 'cache-control': 'private, no-store' }
	});
};
