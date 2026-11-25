import { expect, test } from "bun:test";
import {
  cleanupExpiredEdgeTickets,
  type CleanupLogger,
  type ExpiredTicket,
  type TicketRepository,
} from "../src/cleanup";
import { createScheduledHandler, type Env } from "../src/index";

const now = new Date("2099-01-01T01:00:00.000Z");

type StoredTicket = ExpiredTicket & { expiresAt: string };

class MemoryRepository implements TicketRepository {
  readonly events: string[] = [];
  failDeleteFor = new Set<number>();

  constructor(readonly tickets: StoredTicket[]) {}

  async expiredTickets(deadline: string) {
    this.events.push(`scan:${deadline}`);
    return this.tickets
      .filter((candidate) => candidate.expiresAt <= deadline)
      .map(({ id }) => ({ id }));
  }

  async deleteExpiredTicket(ticketId: number, deadline: string) {
    this.events.push(`delete-ticket:${ticketId}`);
    if (this.failDeleteFor.delete(ticketId)) throw new Error("database_unavailable");
    const index = this.tickets.findIndex(
      (candidate) => candidate.id === ticketId && candidate.expiresAt <= deadline,
    );
    if (index < 0) return false;
    this.tickets.splice(index, 1);
    return true;
  }
}

function logger() {
  const info: Array<Record<string, unknown>> = [];
  const errors: Array<Record<string, unknown>> = [];
  const value: CleanupLogger = {
    info: (entry) => info.push(entry),
    error: (entry) => errors.push(entry),
  };
  return { value, info, errors };
}

test("the scheduled entry cleans stale ACME TXT records, then expired tickets", async () => {
  const repository = new MemoryRepository([]);
  const logs = logger();
  const txtCleanups: Date[] = [];
  const handler = createScheduledHandler({
    now: () => now,
    repository: () => repository,
    dns: () => ({
      async cleanupExpiredTxt(at: Date) {
        txtCleanups.push(at);
        return 2;
      },
    }),
    logger: logs.value,
  });

  await handler({ scheduledTime: 0, cron: "0 * * * *", noRetry() {} }, {} as Env);

  expect(txtCleanups).toEqual([now]);
  expect(repository.events).toEqual([`scan:${now.toISOString()}`]);
  expect(logs.info).toEqual([
    { event: "edge_acme_txt_cleanup", deleted: 2 },
    { event: "edge_ticket_cleanup_finished", scanned: 0, ticketsDeleted: 0, skipped: 0, failed: 0 },
  ]);
});

test("a TXT cleanup failure does not stop ticket cleanup", async () => {
  const repository = new MemoryRepository([{ id: 1, expiresAt: now.toISOString() }]);
  const logs = logger();
  const handler = createScheduledHandler({
    now: () => now,
    repository: () => repository,
    dns: () => ({
      async cleanupExpiredTxt() {
        throw new Error("provider unavailable");
      },
    }),
    logger: logs.value,
  });

  await handler({ scheduledTime: 0, cron: "0 * * * *", noRetry() {} }, {} as Env);

  expect(logs.errors).toEqual([{ event: "edge_acme_txt_cleanup_failed" }]);
  expect(repository.tickets).toEqual([]);
});

test("diagnostic output cannot replace the cleanup result", async () => {
  const result = await cleanupExpiredEdgeTickets(now, new MemoryRepository([]), {
    info() {
      throw new Error("logger unavailable");
    },
    error() {
      throw new Error("logger unavailable");
    },
  });

  expect(result).toEqual({ scanned: 0, ticketsDeleted: 0, skipped: 0, failed: 0 });
});

test("expired tickets of every purpose are deleted and the expiration boundary is inclusive", async () => {
  const repository = new MemoryRepository([
    { id: 1, expiresAt: "2099-01-01T00:00:00.000Z" },
    { id: 2, expiresAt: now.toISOString() },
    { id: 3, expiresAt: "2099-01-01T01:00:00.001Z" },
  ]);
  const logs = logger();

  expect(await cleanupExpiredEdgeTickets(now, repository, logs.value)).toEqual({
    scanned: 2,
    ticketsDeleted: 2,
    skipped: 0,
    failed: 0,
  });
  expect(repository.tickets.map(({ id }) => id)).toEqual([3]);
});

test("one failed deletion is retried later without stopping independent cleanup", async () => {
  const repository = new MemoryRepository([
    { id: 1, expiresAt: now.toISOString() },
    { id: 2, expiresAt: now.toISOString() },
  ]);
  repository.failDeleteFor.add(1);
  const logs = logger();

  expect(await cleanupExpiredEdgeTickets(now, repository, logs.value)).toMatchObject({
    ticketsDeleted: 1,
    failed: 1,
  });
  expect(logs.errors).toEqual([
    { event: "edge_ticket_cleanup_item_failed", ticket_id: 1, error: "unexpected_error" },
  ]);
  expect(await cleanupExpiredEdgeTickets(now, repository, logs.value)).toMatchObject({
    ticketsDeleted: 1,
    failed: 0,
  });
  expect(repository.tickets).toEqual([]);
});

test("a ticket removed by an overlapping run is skipped", async () => {
  const repository = new MemoryRepository([{ id: 1, expiresAt: now.toISOString() }]);
  const scan = repository.expiredTickets.bind(repository);
  repository.expiredTickets = async (deadline) => {
    const tickets = await scan(deadline);
    repository.tickets.splice(0, 1);
    return tickets;
  };

  expect(await cleanupExpiredEdgeTickets(now, repository, logger().value)).toMatchObject({
    ticketsDeleted: 0,
    skipped: 1,
  });
});
