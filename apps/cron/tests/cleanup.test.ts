import { expect, test } from "bun:test";
import {
  cleanupExpiredEdgeTickets,
  type CleanupLogger,
  type DeploymentOwnership,
  type DnsARecord,
  type DnsProvider,
  type ExpiredTicket,
  type TicketRepository,
} from "../src/cleanup";
import { DnsProviderError } from "../src/dns-errors";
import { createScheduledHandler, type Env } from "../src/index";

const now = new Date("2099-01-01T01:00:00.000Z");
const validPayload = JSON.stringify({ name: "brave-silver-atlas" });

function ticket(
  overrides: Partial<ExpiredTicket & { expiresAt: string }> & { id: number },
): ExpiredTicket & { expiresAt: string } {
  return {
    purpose: "edge_deployment",
    expectedEdgeId: `edge-${overrides.id}`,
    payload: validPayload,
    consumedAt: null,
    expiresAt: "2099-01-01T00:00:00.000Z",
    ...overrides,
  };
}

class MemoryRepository implements TicketRepository {
  readonly edges = new Map<string, string>();
  readonly events: string[] = [];
  failDeleteFor = new Set<number>();

  constructor(readonly tickets: Array<ExpiredTicket & { expiresAt: string }>) {}

  async expiredTickets(deadline: string) {
    this.events.push(`scan:${deadline}`);
    return this.tickets.filter((candidate) => candidate.expiresAt <= deadline);
  }

  async registeredEdgeExists(edgeId: string) {
    return this.edges.has(edgeId);
  }

  async deploymentOwnership(
    candidate: ExpiredTicket,
    name: string,
    deadline: string,
  ): Promise<DeploymentOwnership> {
    this.events.push(`ownership:${candidate.id}`);
    return {
      currentTicketExists: this.tickets.some(
        (stored) =>
          stored.id === candidate.id &&
          stored.purpose === "edge_deployment" &&
          stored.expiresAt <= deadline,
      ),
      registeredEdgeExists:
        this.edges.has(candidate.expectedEdgeId) || [...this.edges.values()].includes(name),
      otherDeploymentTicketExists: this.tickets.some((stored) => {
        if (stored.id === candidate.id || stored.purpose !== "edge_deployment") return false;
        try {
          return (JSON.parse(stored.payload) as { name?: unknown }).name === name;
        } catch {
          return false;
        }
      }),
    };
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

class MemoryDns implements DnsProvider {
  readonly events: string[] = [];
  failLookupFor = new Set<string>();

  constructor(readonly records = new Map<string, { id: string }>()) {}

  async cleanupExpiredTxt(_now: Date) {
    return 0;
  }

  async findA(hostname: string) {
    this.events.push(`find:${hostname}`);
    if (this.failLookupFor.has(hostname)) throw new Error("dns_lookup_failed");
    const record = this.records.get(hostname);
    return record ? { ...record, hostname } : null;
  }

  async deleteA(record: DnsARecord) {
    this.events.push(`delete:${record.id}`);
    for (const [hostname, candidate] of this.records) {
      if (candidate.id === record.id) this.records.delete(hostname);
    }
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

test("the scheduled entry uses its actual invocation time", async () => {
  const repository = new MemoryRepository([]);
  const dns = new MemoryDns();
  const logs = logger();
  const handler = createScheduledHandler({
    now: () => now,
    repository: () => repository,
    dns: () => dns,
    logger: logs.value,
  });

  await handler({ scheduledTime: 0, cron: "0 * * * *", noRetry() {} }, {} as Env);

  expect(repository.events).toEqual([`scan:${now.toISOString()}`]);
  expect(logs.info).toEqual([
    { event: "edge_acme_txt_cleanup", deleted: 0 },
    {
      event: "edge_ticket_cleanup_finished",
      scanned: 0,
      ticketsDeleted: 0,
      dnsRecordsDeleted: 0,
      skipped: 0,
      failed: 0,
    },
  ]);
});

test("diagnostic output cannot replace the cleanup result", async () => {
  const result = await cleanupExpiredEdgeTickets(now, new MemoryRepository([]), new MemoryDns(), {
    info() {
      throw new Error("logger unavailable");
    },
    error() {
      throw new Error("logger unavailable");
    },
  });

  expect(result).toEqual({
    scanned: 0,
    ticketsDeleted: 0,
    dnsRecordsDeleted: 0,
    skipped: 0,
    failed: 0,
  });
});

test("ordinary purposes are deleted first and the expiration boundary is inclusive", async () => {
  const repository = new MemoryRepository([
    ticket({ id: 1 }),
    ticket({ id: 2, purpose: "device_tunnel", expiresAt: now.toISOString() }),
    ticket({ id: 3, purpose: "dashboard_access" }),
    ticket({ id: 4, purpose: "device_tunnel", expiresAt: "2099-01-01T01:00:00.001Z" }),
  ]);
  const dns = new MemoryDns();
  const logs = logger();

  const result = await cleanupExpiredEdgeTickets(now, repository, dns, logs.value);

  expect(result).toEqual({
    scanned: 3,
    ticketsDeleted: 3,
    dnsRecordsDeleted: 0,
    skipped: 0,
    failed: 0,
  });
  expect(repository.tickets.map(({ id }) => id)).toEqual([4]);
  expect(repository.events.slice(1, 3)).toEqual(["delete-ticket:2", "delete-ticket:3"]);
});

test("a failed deployment deletes its known DNS record before its ticket", async () => {
  const repository = new MemoryRepository([ticket({ id: 1 })]);
  const dns = new MemoryDns(new Map([["brave-silver-atlas.edge.pontia.dev", { id: "dns-old" }]]));
  const logs = logger();
  const events: string[] = [];
  const originalDeleteA = dns.deleteA.bind(dns);
  dns.deleteA = async (record) => {
    events.push("dns");
    await originalDeleteA(record);
  };
  const originalDeleteTicket = repository.deleteExpiredTicket.bind(repository);
  repository.deleteExpiredTicket = async (id, deadline) => {
    events.push("ticket");
    return originalDeleteTicket(id, deadline);
  };

  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    ticketsDeleted: 1,
    dnsRecordsDeleted: 1,
  });
  expect(events).toEqual(["dns", "ticket"]);
  expect(repository.events.filter((event) => event === "ownership:1")).toHaveLength(2);
});

test("consumed deployments and deployments with a registered edge only lose the ticket", async () => {
  const repository = new MemoryRepository([
    ticket({ id: 1, consumedAt: "2099-01-01T00:30:00.000Z" }),
    ticket({ id: 2 }),
  ]);
  repository.edges.set("edge-2", "brave-silver-atlas");
  const dns = new MemoryDns(new Map([["brave-silver-atlas.edge.pontia.dev", { id: "dns-live" }]]));
  const logs = logger();

  const result = await cleanupExpiredEdgeTickets(now, repository, dns, logs.value);

  expect(result.ticketsDeleted).toBe(2);
  expect(dns.events).toEqual([]);
  expect(dns.records.has("brave-silver-atlas.edge.pontia.dev")).toBe(true);
});

test("registered names and other deployment tickets block DNS deletion", async () => {
  const repository = new MemoryRepository([
    ticket({ id: 1 }),
    ticket({ id: 2, expiresAt: "2099-01-01T02:00:00.000Z" }),
  ]);
  const dns = new MemoryDns(new Map([["brave-silver-atlas.edge.pontia.dev", { id: "dns-live" }]]));
  const logs = logger();

  const result = await cleanupExpiredEdgeTickets(now, repository, dns, logs.value);

  expect(result).toMatchObject({ ticketsDeleted: 0, dnsRecordsDeleted: 0, skipped: 1 });
  expect(dns.events).toEqual([]);
  repository.tickets.pop();
  repository.edges.set("different-edge", "brave-silver-atlas");
  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    ticketsDeleted: 1,
    dnsRecordsDeleted: 0,
  });
});

test("invalid deployment payloads never cause DNS operations", async () => {
  const repository = new MemoryRepository([
    ticket({ id: 1, payload: JSON.stringify({ name: "outside.edge.pontia.dev" }) }),
  ]);
  const dns = new MemoryDns();
  const logs = logger();

  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    ticketsDeleted: 0,
    failed: 1,
  });
  expect(repository.tickets).toHaveLength(1);
  expect(dns.events).toEqual([]);
  expect(logs.errors[0]).toMatchObject({
    event: "edge_ticket_cleanup_item_failed",
    ticket_id: 1,
    error: "invalid_deployment_payload",
  });
});

test("one DNS failure retains its ticket without stopping independent cleanup", async () => {
  const repository = new MemoryRepository([
    ticket({ id: 1 }),
    ticket({
      id: 2,
      payload: JSON.stringify({ name: "silent-crimson-orion" }),
    }),
    ticket({ id: 3, purpose: "dashboard_access" }),
  ]);
  const dns = new MemoryDns();
  dns.failLookupFor.add("brave-silver-atlas.edge.pontia.dev");
  const logs = logger();

  const result = await cleanupExpiredEdgeTickets(now, repository, dns, logs.value);

  expect(result).toMatchObject({ ticketsDeleted: 2, failed: 1 });
  expect(repository.tickets.map(({ id }) => id)).toEqual([1]);
  expect(dns.events).toContain("find:silent-crimson-orion.edge.pontia.dev");
});

test("DNS cleanup logs safe provider diagnostics and retains the ticket", async () => {
  const repository = new MemoryRepository([ticket({ id: 1 })]);
  const dns = new MemoryDns();
  dns.findA = async () => {
    throw new DnsProviderError("lookup", "provider_http_error", 403, [
      { code: 10000, message: "Authentication error" },
    ]);
  };
  const logs = logger();

  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    failed: 1,
    ticketsDeleted: 0,
  });
  expect(logs.errors).toEqual([
    {
      event: "edge_ticket_cleanup_item_failed",
      ticket_id: 1,
      hostname: "brave-silver-atlas.edge.pontia.dev",
      operation: "lookup",
      error: "provider_http_error",
      status: 403,
      provider_errors: [{ code: 10000, message: "Authentication error" }],
    },
  ]);
  expect(repository.tickets).toHaveLength(1);
});

test("a retry finishes ticket deletion after DNS was removed", async () => {
  const repository = new MemoryRepository([ticket({ id: 1 })]);
  repository.failDeleteFor.add(1);
  const dns = new MemoryDns(new Map([["brave-silver-atlas.edge.pontia.dev", { id: "dns-old" }]]));
  const logs = logger();

  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    dnsRecordsDeleted: 1,
    ticketsDeleted: 0,
    failed: 1,
  });
  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    dnsRecordsDeleted: 0,
    ticketsDeleted: 1,
    failed: 0,
  });
});

test("an overlapping cleanup cannot delete DNS after the old ticket is removed", async () => {
  const repository = new MemoryRepository([ticket({ id: 1 })]);
  const dns = new MemoryDns(new Map([["brave-silver-atlas.edge.pontia.dev", { id: "dns-old" }]]));
  const originalFind = dns.findA.bind(dns);
  dns.findA = async (hostname) => {
    const record = await originalFind(hostname);
    repository.tickets.splice(0, 1);
    repository.tickets.push(ticket({ id: 2, expiresAt: "2099-01-01T02:00:00.000Z" }));
    dns.records.set(hostname, { id: "dns-new" });
    return record;
  };
  const logs = logger();

  expect(await cleanupExpiredEdgeTickets(now, repository, dns, logs.value)).toMatchObject({
    dnsRecordsDeleted: 0,
    ticketsDeleted: 0,
    skipped: 1,
  });
  expect(dns.records.get("brave-silver-atlas.edge.pontia.dev")).toEqual({ id: "dns-new" });
  expect(dns.events.some((event) => event.startsWith("delete:"))).toBe(false);
});
