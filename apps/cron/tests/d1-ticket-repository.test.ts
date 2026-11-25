import { afterAll, beforeAll, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { Miniflare } from "miniflare";
import { D1TicketRepository } from "../src/d1-ticket-repository";

const temporaryDirectory = resolve(tmpdir());
const testRootPromise = mkdtemp(join(temporaryDirectory, "pontia-cron-"));
let runtime: Miniflare | undefined;
let db: D1Database;

beforeAll(async () => {
  const testRoot = await testRootPromise;
  runtime = new Miniflare({
    resourcePersistencePath: join(testRoot, "storage"),
    resourceTmpPath: join(testRoot, "tmp"),
    workers: [
      {
        config: {
          name: "cron-test",
          compatibilityDate: "2026-08-11",
          manifest: {
            mainModule: "index.js",
            modulesRoot: testRoot,
            modules: {
              "index.js": {
                type: "esm",
                contents: 'export default { fetch() { return new Response("ok") } }',
              },
            },
          },
          env: { DB: { type: "d1", id: "test-db" } },
        },
      },
    ],
  });
  db = (await runtime.getD1Database("DB")) as unknown as D1Database;
  await db.batch([
    db.prepare(`CREATE TABLE edge_tickets (
      id INTEGER PRIMARY KEY NOT NULL,
      purpose TEXT NOT NULL,
      expected_edge_id TEXT NOT NULL,
      payload TEXT NOT NULL,
      expires_at TEXT NOT NULL,
      consumed_at TEXT
    )`),
    db.prepare("CREATE INDEX idx_edge_tickets_expires_at ON edge_tickets (expires_at)"),
  ]);
});

afterAll(async () => {
  const testRoot = await testRootPromise;
  try {
    await runtime?.dispose();
  } finally {
    if (!testRoot || !resolve(testRoot).startsWith(temporaryDirectory + sep)) {
      // oxlint-disable-next-line no-unsafe-finally -- An unsafe cleanup path must abort deletion.
      throw new Error("Invalid test root");
    }
    await rm(testRoot, { recursive: true, force: true });
  }
});

test("D1 repository applies the inclusive expiration boundary", async () => {
  const insert = db.prepare(
    `INSERT INTO edge_tickets (id, purpose, expected_edge_id, payload, expires_at, consumed_at)
     VALUES (?1, ?2, ?3, ?4, ?5, NULL)`,
  );
  await db.batch([
    db.prepare("DELETE FROM edge_tickets"),
    insert.bind(
      1,
      "edge_deployment",
      "edge-old",
      '{"hostname":"edge.example.com"}',
      "2099-01-01T01:00:00.000Z",
    ),
    insert.bind(2, "device_tunnel", "edge-old", "{}", "2099-01-01T00:30:00.000Z"),
    insert.bind(
      3,
      "edge_deployment",
      "edge-new",
      '{"hostname":"edge.example.com"}',
      "2099-01-01T02:00:00.000Z",
    ),
  ]);
  const repository = new D1TicketRepository(db);
  const deadline = "2099-01-01T01:00:00.000Z";

  expect(await repository.expiredTickets(deadline)).toEqual([{ id: 2 }, { id: 1 }]);
  expect(await repository.deleteExpiredTicket(1, deadline)).toBe(true);
  expect(await repository.deleteExpiredTicket(1, deadline)).toBe(false);
  expect(await repository.deleteExpiredTicket(3, deadline)).toBe(false);
});
