import { json } from '@sveltejs/kit';
import { cliPrincipal, remoteDatabase } from '$lib/server/remote-access/http';
import { registrationEdges } from '$lib/server/remote-access/registration';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async (event) => {
	if (!(await cliPrincipal(event)))
		return json({ error: 'invalid_credentials' }, { status: 401 });
	return json(await registrationEdges(remoteDatabase(event)));
};
