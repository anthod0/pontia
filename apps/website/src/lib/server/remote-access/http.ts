import type { RequestEvent } from '@sveltejs/kit';
import { authenticateCliCredential } from '../auth/cli';
import { environment } from '../auth/http';
import { database } from '../db';

export async function cliPrincipal(event: RequestEvent) {
	const authorization = event.request.headers.get('authorization');
	const match = authorization && /^Bearer ([^\s]+)$/.exec(authorization);
	if (!match) return null;
	return authenticateCliCredential(database(environment(event).DB), match[1]);
}

export function remoteDatabase(event: RequestEvent) {
	return database(environment(event).DB);
}
