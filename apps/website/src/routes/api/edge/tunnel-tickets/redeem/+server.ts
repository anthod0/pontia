import { json } from '@sveltejs/kit';
import {
	bearerCredential,
	remoteDatabase
} from '$lib/server/remote-access/http';
import { authenticateEdgeCredential } from '$lib/server/remote-access/resources';
import { redeemTunnelTicket } from '$lib/server/remote-access/tickets';
import type { RequestHandler } from './$types';

export const POST: RequestHandler = async (event) => {
	try {
		const credential = bearerCredential(event.request);
		const db = remoteDatabase(event);
		const principal =
			credential && (await authenticateEdgeCredential(db, credential));
		if (!principal)
			return json({ error: 'invalid_edge_credentials' }, { status: 401 });

		let body: unknown;
		try {
			body = await event.request.json();
		} catch {
			return json({ error: 'invalid_tunnel_ticket' }, { status: 401 });
		}
		if (
			!body ||
			typeof body !== 'object' ||
			Array.isArray(body) ||
			typeof (body as Record<string, unknown>).ticket !== 'string' ||
			Object.keys(body).some((key) => key !== 'ticket')
		)
			return json({ error: 'invalid_tunnel_ticket' }, { status: 401 });

		const result = await redeemTunnelTicket(
			db,
			principal.edgeId,
			(body as { ticket: string }).ticket
		);
		if (!result)
			return json({ error: 'invalid_tunnel_ticket' }, { status: 401 });
		return json({ device_id: result.deviceId });
	} catch {
		return json({ error: 'service_unavailable' }, { status: 503 });
	}
};
