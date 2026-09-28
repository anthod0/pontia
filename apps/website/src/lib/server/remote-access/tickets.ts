import { and, eq, exists, gt, isNull, lte, sql } from 'drizzle-orm';
import { base64url } from 'jose';
import { v7 as uuidv7 } from 'uuid';
import { sha256Base64url } from '../crypto';
import type { Database } from '../db';
import { devices, tunnelTickets } from '../db/schema';
import { isUuidV7 } from '../uuid';
import { findOwnedDeviceTarget } from './resources';

const TICKET_LIFETIME_MS = 60_000;
const databaseTimestamp = sql<string>`(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))`;

export type IssuedTunnelTicket = {
	ticket: string;
	tunnelUrl: string;
	expiresAt: string;
};

export type IssueTunnelTicketResult =
	| { status: 'issued'; value: IssuedTunnelTicket }
	| { status: 'device_not_found' };

export type TunnelTicketDependencies = {
	now(): Date;
	createId(): string;
	randomBytes(length: number): Uint8Array;
};

const defaultDependencies: TunnelTicketDependencies = {
	now: () => new Date(),
	createId: () => uuidv7(),
	randomBytes: (length) => crypto.getRandomValues(new Uint8Array(length))
};

export function parseTunnelTicket(ticket: string) {
	const match = /^ptt_v1_([0-9a-f-]+)_([A-Za-z0-9_-]{43})$/.exec(ticket);
	if (!match) return null;
	const [, id, secret] = match;
	if (!isUuidV7(id)) return null;
	try {
		const decoded = base64url.decode(secret);
		if (decoded.length !== 32 || base64url.encode(decoded) !== secret)
			return null;
	} catch {
		return null;
	}
	return { id, secret };
}

export async function issueTunnelTicket(
	db: Database,
	userId: string,
	deviceId: string,
	dependencies: TunnelTicketDependencies = defaultDependencies
): Promise<IssueTunnelTicketResult> {
	const target = await findOwnedDeviceTarget(db, userId, deviceId);
	if (!target) return { status: 'device_not_found' };

	const now = dependencies.now();
	const expiresAt = new Date(now.getTime() + TICKET_LIFETIME_MS).toISOString();
	const id = dependencies.createId();
	if (!isUuidV7(id))
		throw new Error('Ticket ID generator returned an invalid UUIDv7');
	const bytes = dependencies.randomBytes(32);
	if (bytes.length !== 32)
		throw new Error('Ticket secret generator returned an invalid length');
	const secret = base64url.encode(bytes);

	await db.insert(tunnelTickets).values({
		id,
		secretHash: await sha256Base64url(secret),
		userId,
		deviceId: target.deviceId,
		edgeId: target.edgeId,
		expiresAt
	});
	await cleanupTunnelTickets(db, now);

	return {
		status: 'issued',
		value: {
			ticket: `ptt_v1_${id}_${secret}`,
			tunnelUrl: target.tunnelUrl,
			expiresAt
		}
	};
}

export async function redeemTunnelTicket(
	db: Database,
	edgeId: string,
	ticket: string
): Promise<{ deviceId: string } | null> {
	const parsed = parseTunnelTicket(ticket);
	if (!parsed) return null;
	const secretHash = await sha256Base64url(parsed.secret);
	const currentBinding = db
		.select({ value: devices.id })
		.from(devices)
		.where(
			and(
				eq(devices.id, tunnelTickets.deviceId),
				eq(devices.userId, tunnelTickets.userId),
				eq(devices.edgeId, tunnelTickets.edgeId)
			)
		);
	const consumed = await db
		.update(tunnelTickets)
		.set({ consumedAt: databaseTimestamp })
		.where(
			and(
				eq(tunnelTickets.id, parsed.id),
				eq(tunnelTickets.secretHash, secretHash),
				eq(tunnelTickets.edgeId, edgeId),
				isNull(tunnelTickets.consumedAt),
				gt(tunnelTickets.expiresAt, databaseTimestamp),
				exists(currentBinding)
			)
		)
		.returning({ deviceId: tunnelTickets.deviceId })
		.get();
	return consumed ?? null;
}

async function cleanupTunnelTickets(db: Database, now = new Date()) {
	await db
		.delete(tunnelTickets)
		.where(lte(tunnelTickets.expiresAt, now.toISOString()));
}
