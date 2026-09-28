import { expect, test } from 'bun:test';
import { eq } from 'drizzle-orm';
import { base64url } from 'jose';
import {
	authenticateEdgeCredential,
	deviceBindingIsCurrent,
	findOwnedDeviceTarget
} from '../src/lib/server/remote-access/resources';
import { devices, edges, users } from '../src/lib/server/db/schema';
import { testDatabase } from './database';

const database = testDatabase();
const secret = base64url.encode(new Uint8Array(32).fill(7));

async function hash(value: string) {
	return base64url.encode(
		new Uint8Array(
			await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))
		)
	);
}

async function insertEdge(id: string, credentialSecret = secret) {
	await database.db.insert(edges).values({
		id,
		name: `Edge ${id}`,
		tunnelUrl: `wss://${id}.example.com/tunnel`,
		serviceCredentialHash: await hash(credentialSecret)
	});
}

async function insertUserAndDevice(
	userId: string,
	deviceId: string,
	edgeId: string
) {
	await database.db.insert(users).values({ id: userId });
	await database.db.insert(devices).values({ id: deviceId, userId, edgeId });
}

test('edge credentials authenticate only a strict matching credential', async () => {
	await insertEdge('edge-auth');
	await insertEdge('edge_auth_extra');
	const credential = `pec_v1_edge-auth_${secret}`;

	expect(await authenticateEdgeCredential(database.db, credential)).toEqual({
		edgeId: 'edge-auth'
	});
	expect(
		await authenticateEdgeCredential(
			database.db,
			`pec_v1_edge-auth_${base64url.encode(new Uint8Array(32).fill(8))}`
		)
	).toBeNull();
	expect(
		await authenticateEdgeCredential(database.db, `pec_v2_edge-auth_${secret}`)
	).toBeNull();
	expect(
		await authenticateEdgeCredential(database.db, `pec_v1_unknown_${secret}`)
	).toBeNull();
	expect(
		await authenticateEdgeCredential(database.db, `${credential}_extra`)
	).toBeNull();
	expect(
		await authenticateEdgeCredential(
			database.db,
			`pec_v1_edge_auth_extra_${secret}`
		)
	).toBeNull();
});

test('owned device lookup returns its current edge target', async () => {
	await insertEdge('edge-target');
	await insertUserAndDevice('user-owner', 'device-owned', 'edge-target');
	await database.db.insert(users).values({ id: 'user-other' });

	expect(
		await findOwnedDeviceTarget(database.db, 'user-owner', 'device-owned')
	).toEqual({
		deviceId: 'device-owned',
		edgeId: 'edge-target',
		tunnelUrl: 'wss://edge-target.example.com/tunnel'
	});
	expect(
		await findOwnedDeviceTarget(database.db, 'user-other', 'device-owned')
	).toBeNull();
	expect(
		await findOwnedDeviceTarget(database.db, 'user-owner', 'device-missing')
	).toBeNull();
});

test('device binding checks owner, device, and edge together', async () => {
	await insertEdge('edge-binding');
	await insertEdge('edge-other');
	await insertUserAndDevice('user-binding', 'device-binding', 'edge-binding');

	expect(
		await deviceBindingIsCurrent(
			database.db,
			'user-binding',
			'device-binding',
			'edge-binding'
		)
	).toBe(true);
	expect(
		await deviceBindingIsCurrent(
			database.db,
			'user-binding',
			'device-binding',
			'edge-other'
		)
	).toBe(false);
	expect(
		await deviceBindingIsCurrent(
			database.db,
			'user-other',
			'device-binding',
			'edge-binding'
		)
	).toBe(false);
});

test('device foreign keys cascade owners and restrict deleting assigned edges', async () => {
	await insertEdge('edge-constraints');
	await insertUserAndDevice(
		'user-constraints',
		'device-constraints',
		'edge-constraints'
	);

	await expect(
		Promise.resolve(
			database.db.delete(edges).where(eq(edges.id, 'edge-constraints'))
		)
	).rejects.toThrow();
	await database.db.delete(users).where(eq(users.id, 'user-constraints'));
	expect(
		await database.db
			.select()
			.from(devices)
			.where(eq(devices.id, 'device-constraints'))
	).toHaveLength(0);
	await expect(
		Promise.resolve(
			database.db.delete(edges).where(eq(edges.id, 'edge-constraints'))
		)
	).resolves.toBeDefined();
});
