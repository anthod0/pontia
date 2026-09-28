import { asc, eq, sql } from 'drizzle-orm';
import type { Database } from '../db';
import { devices, edges } from '../db/schema';
import { isUuidV7 } from '../uuid';

export type RegistrationEdge = {
	id: string;
	name: string;
};

export type RegisteredDevice = {
	id: string;
	name: string | null;
	edgeId: string;
	edgeName: string;
};

export type DeviceLookup =
	| { status: 'found'; device: RegisteredDevice }
	| { status: 'not_found' }
	| { status: 'conflict' };

export type DeviceRegistration =
	| { status: 'created' | 'existing'; device: RegisteredDevice }
	| { status: 'invalid_request' | 'edge_not_found' | 'conflict' };

function validName(value: string) {
	return (
		value.trim() === value &&
		value.length > 0 &&
		!/[\u0000-\u001f\u007f-\u009f]/.test(value)
	);
}

export async function registrationEdges(
	db: Database
): Promise<RegistrationEdge[]> {
	return db
		.select({ id: edges.id, name: edges.name })
		.from(edges)
		.orderBy(asc(edges.name), asc(edges.id));
}

type StoredDevice = RegisteredDevice & { userId: string };

function registeredDevice(device: StoredDevice): RegisteredDevice {
	return {
		id: device.id,
		name: device.name,
		edgeId: device.edgeId,
		edgeName: device.edgeName
	};
}

async function storedDevice(
	db: Database,
	deviceId: string
): Promise<StoredDevice | null> {
	const device = await db
		.select({
			id: devices.id,
			name: devices.name,
			userId: devices.userId,
			edgeId: devices.edgeId,
			edgeName: edges.name
		})
		.from(devices)
		.innerJoin(edges, eq(devices.edgeId, edges.id))
		.where(eq(devices.id, deviceId))
		.get();
	return device ?? null;
}

export async function findRegisteredDevice(
	db: Database,
	userId: string,
	deviceId: string
): Promise<DeviceLookup> {
	if (!isUuidV7(deviceId)) return { status: 'not_found' };
	const device = await storedDevice(db, deviceId);
	if (!device) return { status: 'not_found' };
	if (device.userId !== userId) return { status: 'conflict' };
	return { status: 'found', device: registeredDevice(device) };
}

export async function registerDevice(
	db: Database,
	userId: string,
	deviceId: string,
	name: string,
	edgeId: string
): Promise<DeviceRegistration> {
	if (!isUuidV7(deviceId) || !isUuidV7(edgeId) || !validName(name))
		return { status: 'invalid_request' };
	const now = new Date().toISOString();
	const created = await db
		.insert(devices)
		.select(
			db
				.select({
					id: sql<string>`${deviceId}`.as('id'),
					userId: sql<string>`${userId}`.as('user_id'),
					edgeId: edges.id,
					name: sql<string>`${name}`.as('name'),
					createdAt: sql<string>`${now}`.as('created_at'),
					updatedAt: sql<string>`${now}`.as('updated_at')
				})
				.from(edges)
				.where(eq(edges.id, edgeId))
		)
		.onConflictDoNothing()
		.returning({ id: devices.id });
	const stored = await storedDevice(db, deviceId);
	if (!stored) return { status: 'edge_not_found' };
	if (stored.userId !== userId || stored.edgeId !== edgeId)
		return { status: 'conflict' };
	return {
		status: created.length === 1 ? 'created' : 'existing',
		device: registeredDevice(stored)
	};
}
