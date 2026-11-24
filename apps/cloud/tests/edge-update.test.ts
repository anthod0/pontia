import { expect, spyOn, test } from "bun:test";
import { base64url, exportJWK, generateKeyPair, jwtVerify, SignJWT } from "jose";
import { edges, users } from "../src/lib/server/db/schema";
import { notifyEdgeUpdates } from "../src/lib/server/edge-update";
import { testDatabase } from "./database";

const database = testDatabase();

test("notifies reachable outdated Edges with a signed one-use challenge and continues after failures", async () => {
  const { privateKey, publicKey } = await generateKeyPair("EdDSA", { extractable: true });
  const key = base64url.encode(new Uint8Array(await crypto.subtle.exportKey("pkcs8", privateKey)));
  const names = ["accepted", "timeout", "current", "offline", "legacy", "last"];
  const rows = names.map((name, index) => ({
    id: "0199791c-6600-7000-8000-00000000000" + index,
    userId: "owner",
    name,
    dnsLabel: name,
    serviceCredentialHash: name,
    tunnelUrl: "wss://" + name + ".edge.pontia.dev/tunnel",
  }));
  await database.db.insert(users).values({ id: "owner" });
  await database.db.insert(edges).values(rows);
  const nonce = crypto.randomUUID();
  const tokens = new Map<string, string>();
  const fetcher = async (target: string, init: RequestInit) => {
    const url = new URL(target);
    const name = url.hostname.split(".")[0];
    if (url.pathname === "/healthz") {
      if (name === "offline") throw new Error("offline");
      return new Response("ok", {
        headers: {
          "pontia-edge-version": name === "current" ? "2.0.0" : "1.0.0",
          ...(name === "legacy" ? {} : { "pontia-update-nonce": nonce }),
        },
      });
    }
    if (name === "timeout") throw new DOMException("timed out", "TimeoutError");
    if (
      url.pathname !== "/__pontia/notify-update" ||
      init.method !== "POST" ||
      init.redirect !== "error"
    )
      return new Response(null, { status: 400 });
    tokens.set(name, new Headers(init.headers).get("authorization")!.slice(7));
    return new Response(null, { status: 202 });
  };
  const first = await notifyEdgeUpdates(database.db, "", "2.0.0", key, fetcher);
  expect(first).toEqual({ next: rows[4].id, accepted: 1, failed: 1, skipped: 3 });
  expect(await notifyEdgeUpdates(database.db, first.next!, "2.0.0", key, fetcher)).toEqual({
    next: null,
    accepted: 1,
    failed: 0,
    skipped: 0,
  });
  for (const row of [rows[0], rows[5]]) {
    const { payload } = await jwtVerify(tokens.get(row.name)!, publicKey, {
      audience: row.id,
      typ: "pontia-edge-update+jwt",
      algorithms: ["EdDSA"],
    });
    expect(payload.jti).toBe(nonce);
    expect(payload.exp! - payload.iat!).toBe(60);
  }
});

test("only the main release workflow can request a notification page", async () => {
  const { POST } = await import("../src/routes/api/internal/notify-update/+server");
  const { privateKey, publicKey } = await generateKeyPair("RS256");
  const jwk = await exportJWK(publicKey);
  const mockFetch = spyOn(globalThis, "fetch").mockResolvedValue(
    Response.json({ keys: [{ ...jwk, kid: "github" }] }),
  );
  const url = new URL("https://pontia.dev/api/internal/notify-update");
  const call = (token?: string) =>
    POST({
      request: new Request(url, {
        method: "POST",
        headers: token ? { Authorization: "Bearer " + token } : {},
      }),
      url,
    } as Parameters<typeof POST>[0]);
  const claims = {
    repository: "anthod0/pontia",
    workflow_ref: "anthod0/pontia/.github/workflows/release.yml@refs/heads/main",
    ref: "refs/heads/main",
    event_name: "push",
  };
  const sign = (custom: Record<string, string>, audience = url.href) =>
    new SignJWT({ ...claims, ...custom })
      .setProtectedHeader({ alg: "RS256", kid: "github" })
      .setIssuer("https://token.actions.githubusercontent.com")
      .setAudience(audience)
      .setIssuedAt()
      .setExpirationTime("5m")
      .sign(privateKey);
  try {
    expect((await call()).status).toBe(401);
    expect((await call(await sign({}, "some-other-service"))).status).toBe(401);
    for (const custom of Array.of<Record<string, string>>(
      { workflow_ref: "another-workflow" },
      { ref: "refs/heads/feature" },
      { repository: "someone/pontia" },
      { event_name: "pull_request_target" },
    ))
      expect((await call(await sign(custom))).status).toBe(403);
    // An authorized caller reaches request validation (missing release version).
    expect((await call(await sign({}))).status).toBe(400);
  } finally {
    mockFetch.mockRestore();
  }
});
