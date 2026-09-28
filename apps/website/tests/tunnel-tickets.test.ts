import { expect, test } from 'bun:test';
import { eq } from 'drizzle-orm';
import { base64url } from 'jose';
import { sha256Base64url } from '../src/lib/server/crypto';
import {
	devices,
	edges,
	tunnelTickets,
	users
} from '../src/lib/server/db/schema';
import {
	issueTunnelTicket,
	parseTunnelTicket,
	redeemTunnelTicket,
	type TunnelTicketDependencies
} from '../src/lib/server/remote-access/tickets';
import { testDatabase } from './database';

const database = testDatabase();
const now = new Date('2026-09-24T12:00:00.000Z');
const redemptionTime = new Date('2099-01-01T00:00:00.000Z');
const ticketId = '0199791c-6600-7000-8000-000000000001';
const secretBytes = new Uint8Array(32).fill(23);

function dependencies(
	id = ticketId,
	currentTime = now
): TunnelTicketDependencies {
	return {
		now: () => currentTime,
		createId: () => id,
		randomBytes: () => secretBytes.slice()
	};
}

async function seedTicketFixture() {
	await database.db
		.insert(users)
		.values([{ id: 'user-owner' }, { id: 'user-other' }]);
	await database.db.insert(edges).values([
		{
			id: 'edge-target',
			name: 'Target Edge',
			tunnelUrl: 'wss://target.example/tunnel',
			serviceCredentialHash: 'target-hash'
		},
		{
			id: 'edge-other',
			name: 'Other Edge',
			tunnelUrl: 'wss://other.example/tunnel',
			serviceCredentialHash: 'other-hash'
		}
	]);
	await database.db.insert(devices).values({
		id: 'device-owned',
		userId: 'user-owner',
		edgeId: 'edge-target'
	});
}

async function issuedTicket(id = ticketId) {
	const result = await issueTunnelTicket(
		database.db,
		'user-owner',
		'device-owned',
		dependencies(id, redemptionTime)
	);
	if (result.status !== 'issued') throw new Error('Expected a ticket');
	return result.value;
}

test('ticket parsing accepts only canonical v1 tickets', () => {
	const secret = base64url.encode(secretBytes);
	const ticket = `ptt_v1_${ticketId}_${secret}`;

	expect(parseTunnelTicket(ticket)).toEqual({ id: ticketId, secret });
	expect(parseTunnelTicket(ticket.replace('ptt_v1', 'ptt_v2'))).toBeNull();
	expect(parseTunnelTicket(`${ticket}x`)).toBeNull();
	expect(parseTunnelTicket(ticket.replace(ticketId, 'not-a-uuid'))).toBeNull();
	expect(
		parseTunnelTicket(ticket.replace(secret, `${secret.slice(0, -1)}=`))
	).toBeNull();
});

test('issuing binds an opaque 60 second ticket without storing its secret', async () => {
	await seedTicketFixture();

	const result = await issueTunnelTicket(
		database.db,
		'user-owner',
		'device-owned',
		dependencies()
	);

	expect(result).toEqual({
		status: 'issued',
		value: {
			ticket: `ptt_v1_${ticketId}_${base64url.encode(secretBytes)}`,
			tunnelUrl: 'wss://target.example/tunnel',
			expiresAt: '2026-09-24T12:01:00.000Z'
		}
	});
	const stored = await database.db.select().from(tunnelTickets).get();
	expect(stored).toMatchObject({
		id: ticketId,
		userId: 'user-owner',
		deviceId: 'device-owned',
		edgeId: 'edge-target',
		expiresAt: '2026-09-24T12:01:00.000Z',
		consumedAt: null,
		secretHash: await sha256Base64url(base64url.encode(secretBytes))
	});
	expect(JSON.stringify(stored)).not.toContain(base64url.encode(secretBytes));
});

test('issuing hides devices not owned by the authenticated user', async () => {
	await seedTicketFixture();

	expect(
		await issueTunnelTicket(
			database.db,
			'user-other',
			'device-owned',
			dependencies()
		)
	).toEqual({ status: 'device_not_found' });
	expect(
		await issueTunnelTicket(
			database.db,
			'user-owner',
			'device-missing',
			dependencies()
		)
	).toEqual({ status: 'device_not_found' });
});

test('a ticket can be redeemed once only by its bound edge', async () => {
	await seedTicketFixture();
	const issued = await issuedTicket();

	expect(
		await redeemTunnelTicket(database.db, 'edge-other', issued.ticket)
	).toBeNull();
	expect(
		await redeemTunnelTicket(database.db, 'edge-target', issued.ticket)
	).toEqual({ deviceId: 'device-owned' });
	expect(
		await redeemTunnelTicket(database.db, 'edge-target', issued.ticket)
	).toBeNull();
});

test('redemption rejects wrong secrets, expired tickets, and stale bindings', async () => {
	await seedTicketFixture();
	const wrongSecret = await issuedTicket(
		'0199791c-6600-7000-8000-000000000002'
	);
	const expired = await issuedTicket('0199791c-6600-7000-8000-000000000003');
	const stale = await issuedTicket('0199791c-6600-7000-8000-000000000004');
	const replacementSecret = base64url.encode(new Uint8Array(32).fill(24));

	expect(
		await redeemTunnelTicket(
			database.db,
			'edge-target',
			wrongSecret.ticket.replace(
				base64url.encode(secretBytes),
				replacementSecret
			)
		)
	).toBeNull();
	await database.db
		.update(tunnelTickets)
		.set({ expiresAt: '2000-01-01T00:00:00.000Z' })
		.where(eq(tunnelTickets.id, parseTunnelTicket(expired.ticket)!.id));
	expect(
		await redeemTunnelTicket(database.db, 'edge-target', expired.ticket)
	).toBeNull();
	await database.db
		.update(devices)
		.set({ edgeId: 'edge-other' })
		.where(eq(devices.id, 'device-owned'));
	expect(
		await redeemTunnelTicket(database.db, 'edge-target', stale.ticket)
	).toBeNull();
});

test('concurrent redemption consumes a ticket at most once', async () => {
	await seedTicketFixture();
	const issued = await issuedTicket();

	const results = await Promise.all(
		Array.from({ length: 8 }, () =>
			redeemTunnelTicket(database.db, 'edge-target', issued.ticket)
		)
	);

	expect(results.filter((result) => result !== null)).toEqual([
		{ deviceId: 'device-owned' }
	]);
});

test('issuing removes expired ticket records', async () => {
	await seedTicketFixture();
	await database.db.insert(tunnelTickets).values({
		id: '0199791c-6600-7000-8000-000000000099',
		secretHash: 'expired-hash',
		userId: 'user-owner',
		deviceId: 'device-owned',
		edgeId: 'edge-target',
		expiresAt: now.toISOString()
	});

	await issuedTicket();

	expect(
		await database.db
			.select({ id: tunnelTickets.id })
			.from(tunnelTickets)
			.where(eq(tunnelTickets.id, '0199791c-6600-7000-8000-000000000099'))
			.get()
	).toBeUndefined();
});
