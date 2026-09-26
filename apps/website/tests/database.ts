import { afterAll, beforeAll, beforeEach } from 'bun:test';
import { mkdtemp, readdir, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { Miniflare } from 'miniflare';
import { database, type Database } from '../src/lib/server/db';
import { users } from '../src/lib/server/db/schema';

export function testDatabase() {
	let binding: D1Database;
	let db: Database;
	const testRootPromise = mkdtemp(join(tmpdir(), 'pontia-auth-'));
	let runtime: Miniflare | undefined;
	afterAll(async () => {
		const testRoot = await testRootPromise;
		try {
			await runtime?.dispose();
		} finally {
			if (!testRoot || !resolve(testRoot).startsWith(resolve(tmpdir()) + sep))
				throw new Error('Invalid test root');
			await rm(testRoot, { recursive: true, force: true });
		}
	});
	beforeAll(async () => {
		const testRoot = await testRootPromise;
		runtime = new Miniflare({
			resourcePersistencePath: join(testRoot, 'storage'),
			resourceTmpPath: join(testRoot, 'tmp'),
			workers: [
				{
					config: {
						name: 'auth-test',
						type: 'worker',
						compatibilityDate: '2026-08-11',
						manifest: {
							mainModule: 'index.js',
							modulesRoot: testRoot,
							modules: {
								'index.js': {
									type: 'esm',
									contents:
										'export default { fetch() { return new Response("ok") } }'
								}
							}
						},
						env: { DB: { type: 'd1', id: 'test-db' } }
					}
				}
			]
		});
		binding = (await runtime.getD1Database('DB')) as unknown as D1Database;
		const migrationRoot = new URL('../migrations/', import.meta.url);
		for (const directory of (await readdir(migrationRoot)).sort()) {
			const migration = await readFile(
				new URL(`${directory}/migration.sql`, migrationRoot),
				'utf8'
			);
			await binding.batch(
				migration
					.split('--> statement-breakpoint')
					.map((statement) => binding.prepare(statement.trim()))
			);
		}
		db = database(binding);
	}, 30_000);
	beforeEach(async () => {
		await db.delete(users);
	});
	return {
		get db() {
			return db;
		},
		get binding() {
			return binding;
		}
	};
}
