import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import {
  buildEdgeLookupSql,
  buildSetAccessSql,
  parseCommandInput,
  runCommand,
  type AccessScope,
  type EdgeRecord,
} from "../../../scripts/set-edge-access";

const EDGE_ID = "01a10228-8f4f-76c4-a552-947350707111";
const OTHER_ID = "01a11c7f-33a0-77d4-a3e8-3508708c3e81";

async function changeAccess(
  initial: AccessScope,
  target: AccessScope,
  answer: string,
  exists = true,
  tunnelUrl = "wss://edge.example.com/tunnel",
) {
  const db = new Database(":memory:");
  try {
    db.exec(`CREATE TABLE edges (
      id TEXT PRIMARY KEY, name TEXT, tunnel_url TEXT, access_scope TEXT, updated_at TEXT
    )`);
    const insert = db.prepare("INSERT INTO edges VALUES (?, 'edge', ?, ?, 'before')");
    if (exists) insert.run(EDGE_ID, tunnelUrl, initial);
    insert.run(OTHER_ID, "wss://other.example.com/tunnel", initial);
    let confirmations = 0;
    let closed = false;
    const errors: string[] = [];
    const result = await runCommand(parseCommandInput([EDGE_ID, target]), {
      d1: {
        async findEdge(edgeId) {
          const row = db.query(buildEdgeLookupSql(edgeId)).get() as {
            id: string;
            name: string;
            tunnel_url: string;
            access_scope: AccessScope;
          } | null;
          if (!row) return null;
          return {
            id: row.id,
            name: row.name,
            tunnelUrl: row.tunnel_url,
            accessScope: row.access_scope,
          } satisfies EdgeRecord;
        },
        async setEdgeAccess(edgeId, expectedTunnelUrl, scope) {
          db.query(buildSetAccessSql(edgeId, expectedTunnelUrl, scope)).all();
        },
      },
      prompter: {
        async question() {
          confirmations += 1;
          return answer;
        },
        close() {
          closed = true;
        },
      },
      write() {},
      writeError(message) {
        errors.push(message);
      },
    });
    return {
      result,
      confirmations,
      closed,
      errors,
      edge: db.query("SELECT access_scope, updated_at FROM edges WHERE id = ?").get(EDGE_ID),
      other: db.query("SELECT access_scope, updated_at FROM edges WHERE id = ?").get(OTHER_ID),
    };
  } finally {
    db.close();
  }
}

describe("edge access command", () => {
  for (const [initial, target] of [
    ["public", "private"],
    ["private", "public"],
  ] as const) {
    test(`changes only the selected edge from ${initial} to ${target} after confirmation`, async () => {
      const outcome = await changeAccess(initial, target, "yes");
      expect(outcome.result).toBe(0);
      expect(outcome.confirmations).toBe(1);
      expect(outcome.edge).toEqual({
        access_scope: target,
        updated_at: expect.not.stringMatching(/^before$/),
      });
      expect(outcome.other).toEqual({ access_scope: initial, updated_at: "before" });
      expect(outcome.closed).toBe(true);
    });

    test(`keeps ${initial} access when confirmation is declined`, async () => {
      const outcome = await changeAccess(initial, target, "no");
      expect(outcome.result).toBe(0);
      expect(outcome.confirmations).toBe(1);
      expect(outcome.edge).toEqual({ access_scope: initial, updated_at: "before" });
      expect(outcome.closed).toBe(true);
    });

    test(`does not prompt or update when already ${target}`, async () => {
      const outcome = await changeAccess(target, target, "yes");
      expect(outcome.result).toBe(0);
      expect(outcome.confirmations).toBe(0);
      expect(outcome.edge).toEqual({ access_scope: target, updated_at: "before" });
    });
  }

  test("rejects a missing edge without prompting or modifying another edge", async () => {
    const outcome = await changeAccess("public", "private", "yes", false);
    expect(outcome.result).toBe(1);
    expect(outcome.confirmations).toBe(0);
    expect(outcome.edge).toBeNull();
    expect(outcome.other).toEqual({ access_scope: "public", updated_at: "before" });
    expect(outcome.closed).toBe(true);
  });

  test.each([
    "wss://8.8.8.8/tunnel",
    "wss://8.8.8.8:8443/tunnel",
    "wss://[2606:4700:4700::1111]/tunnel",
    "wss://[2606:4700:4700::1111]:8443/tunnel",
  ])("keeps an IP-address edge private: %s", async (tunnelUrl) => {
    const outcome = await changeAccess("private", "public", "yes", true, tunnelUrl);

    expect(outcome.result).toBe(1);
    expect(outcome.confirmations).toBe(0);
    expect(outcome.errors).toHaveLength(1);
    expect(outcome.edge).toEqual({ access_scope: "private", updated_at: "before" });
    expect(outcome.closed).toBe(true);
  });

  test("allows an existing public IP-address edge to be made private", async () => {
    const outcome = await changeAccess("public", "private", "yes", true, "wss://8.8.8.8/tunnel");

    expect(outcome.result).toBe(0);
    expect(outcome.confirmations).toBe(1);
    expect(outcome.edge).toEqual({
      access_scope: "private",
      updated_at: expect.not.stringMatching(/^before$/),
    });
  });

  test("reports an existing public IP-address edge as invalid", async () => {
    const outcome = await changeAccess(
      "public",
      "public",
      "yes",
      true,
      "wss://[2606:4700:4700::1111]/tunnel",
    );

    expect(outcome.result).toBe(1);
    expect(outcome.confirmations).toBe(0);
    expect(outcome.errors).toHaveLength(1);
    expect(outcome.edge).toEqual({ access_scope: "public", updated_at: "before" });
  });

  for (const args of [
    [EDGE_ID],
    [EDGE_ID, "invalid"],
    ["invalid", "private"],
    [EDGE_ID, "private", "extra"],
  ]) {
    test(`rejects invalid arguments ${args.join(" ")}`, () => {
      expect(() => parseCommandInput(args)).toThrow();
    });
  }
});
