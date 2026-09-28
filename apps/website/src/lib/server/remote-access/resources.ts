import { and, eq } from 'drizzle-orm';
import { base64url } from 'jose';
import { sha256Base64url } from '../crypto';
import type { Database } from '../db';
import { devices, edges } from '../db/schema';

export type EdgePrincipal = {
	edgeId: string;
};

export type DeviceTarget = {
	deviceId: string;
	edgeId: string;
	tunnelUrl: string;
};

function parseCredential(credential: string) {
	const match = /^pec_v1_([A-Za-z0-9-]+)_([A-Za-z0-9_-]{43})$/.exec(credential);
	if (!match) return null;
	const [, edgeId, secret] = match;

	try {
		const decoded = base64url.decode(secret);
		if (decoded.length !== 32 || base64url.encode(decoded) !== secret)
			return null;
	} catch {
		return null;
	}
	return { edgeId, secret };
}

export async function authenticateEdgeCredential(
	db: Database,
	credential: string
): Promise<EdgePrincipal | null> {
	const parsed = parseCredential(credential);
	if (!parsed) return null;
	const edge = await db
		.select({
			edgeId: edges.id,
			serviceCredentialHash: edges.serviceCredentialHash
		})
		.from(edges)
		.where(eq(edges.id, parsed.edgeId))
		.get();
	if (
		!edge ||
		edge.serviceCredentialHash !== (await sha256Base64url(parsed.secret))
	)
		return null;
	return { edgeId: edge.edgeId };
}

export async function findOwnedDeviceTarget(
	db: Database,
	userId: string,
	deviceId: string
): Promise<DeviceTarget | null> {
	const target = await db
		.select({
			deviceId: devices.id,
			edgeId: edges.id,
			tunnelUrl: edges.tunnelUrl
		})
		.from(devices)
		.innerJoin(edges, eq(devices.edgeId, edges.id))
		.where(and(eq(devices.id, deviceId), eq(devices.userId, userId)))
		.get();
	return target ?? null;
}

export async function deviceBindingIsCurrent(
	db: Database,
	userId: string,
	deviceId: string,
	edgeId: string
): Promise<boolean> {
	const binding = await db
		.select({ deviceId: devices.id })
		.from(devices)
		.innerJoin(edges, eq(devices.edgeId, edges.id))
		.where(
			and(
				eq(devices.id, deviceId),
				eq(devices.userId, userId),
				eq(devices.edgeId, edgeId)
			)
		)
		.get();
	return binding !== undefined;
}
