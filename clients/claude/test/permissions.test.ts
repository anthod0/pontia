import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import { Host } from "../src/host.js";
import type { HostEvent } from "../src/facts.js";
import { sdk, first, init, assistant, result, settle } from "./fixture.js";

async function setup(t: TestContext) {
  const fixture = sdk();
  const events: HostEvent[] = [];
  const host = new Host(
    { session_id: "pontia-session", cwd: "/unused-fixture-workspace" },
    (e) => events.push(e),
    fixture.factory,
  );
  t.after(async () => {
    if (fixture.messages.failure) await assert.rejects(host.close());
    else await host.close();
  });
  host.submit(first, "task");
  await fixture.next();
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  await settle();
  let requestNumber = 0;
  const request = (agentID?: string) => {
    const abort = new AbortController();
    const response = fixture.options!.canUseTool!(
      "Write",
      { secret: "not for storage" },
      {
        signal: abort.signal,
        toolUseID: "tool-call",
        requestId: `request-${++requestNumber}`,
        agentID,
      },
    );
    const event = events.at(-1)!;
    assert.equal(event.kind, "permission_requested");
    return { abort, response, id: event.permission_id };
  };
  return { fixture, events, host, request };
}

test("parallel approvals return only the selected callback and never change tool input or permissions", async (t) => {
  const { fixture, events, host, request } = await setup(t);
  const a = request();
  const b = request("background-agent");
  let aDone = false;
  void a.response.then(() => {
    aDone = true;
  });
  await settle();
  assert.equal(aDone, false);
  assert.equal(JSON.stringify(events).includes("not for storage"), false);
  assert.deepEqual(
    events.filter((e) => e.kind === "permission_requested").map((e) => e.blocking),
    [true, false],
  );
  assert.throws(() => host.resolvePermission(a.id, "always"));
  host.resolvePermission(b.id, "deny");
  assert.equal((await b.response)?.behavior, "deny");
  assert.equal(aDone, false);
  host.resolvePermission(a.id, "allow");
  assert.deepEqual(await a.response, { behavior: "allow" });
  assert.throws(() => host.resolvePermission(a.id, "deny"));
  assert.equal(events.filter((e) => e.kind === "started").length, 1);
  assert.equal(events.filter((e) => e.kind === "finished").length, 0);
  assert.equal(fixture.calls, 1);
});

test("SDK cancellation expires only its callback and rejects late approval", async (t) => {
  const { events, host, request } = await setup(t);
  const a = request();
  const b = request();
  a.abort.abort();
  assert.equal((await a.response)?.behavior, "deny");
  assert.throws(() => host.resolvePermission(a.id, "allow"));
  host.resolvePermission(b.id, "allow");
  assert.equal((await b.response)?.behavior, "allow");
  assert.deepEqual(
    events.filter((e) => e.kind === "permission_settled").map((e) => e.outcome),
    ["expired", "resolved"],
  );
});

test("pending approval leaves output and interrupt live without fabricating a Turn result", async (t) => {
  const { fixture, events, host, request } = await setup(t);
  const a = request();
  fixture.messages.push(assistant(first, [{ type: "text", text: "still streaming" }], "later"));
  fixture.onInterrupt(async () => a.abort.abort());
  await host.interrupt(first);
  await settle();
  assert.equal((await a.response)?.behavior, "deny");
  assert.ok(events.some((e) => e.kind === "text" && e.delta === "still streaming"));
  assert.equal(events.filter((e) => e.kind === "finished").length, 0);
  fixture.messages.push(result());
  await settle();
  assert.deepEqual(
    events.filter((e) => e.kind === "finished").map((e) => e.outcome),
    ["completed"],
  );
});

test("a disconnected host keeps existing and new SDK callbacks until control returns", async (t) => {
  const { host, request } = await setup(t);
  const first = request();
  host.controlLost();
  const second = request("background-agent");
  host.controlRestored();
  for (const pending of [first, second]) {
    host.resolvePermission(pending.id, "allow");
    assert.equal((await pending.response)?.behavior, "allow");
  }
});

for (const action of ["close", "sdk-failure"] as const) {
  test(`${action} expires approvals without allowing replay`, async (t) => {
    const { fixture, host, request } = await setup(t);
    const a = request();
    if (action === "close") await host.close();
    else {
      fixture.messages.failure = new Error("fixture failure");
      fixture.messages.end();
      await settle();
    }
    assert.equal((await a.response)?.behavior, "deny");
    assert.throws(() => host.resolvePermission(a.id, "allow"));
    assert.equal(host.status().finished, false);
  });
}

test("background callback after a foreground result is not attached to a new user input", async (t) => {
  const { fixture, events, host, request } = await setup(t);
  fixture.messages.push(result());
  await settle();
  const a = request("background-agent");
  const event = events.at(-1)!;
  assert.equal(event.kind, "permission_requested");
  assert.equal("input_id" in event, false);
  host.resolvePermission(a.id, "deny");
  assert.equal((await a.response)?.behavior, "deny");
  assert.equal(events.filter((e) => e.kind === "started").length, 1);
});

test("reordered tool arguments reuse the wait and changed authorization denies both callbacks", async (t) => {
  const { fixture, events, host } = await setup(t);
  const opts = { signal: new AbortController().signal, requestId: "same", toolUseID: "call" };
  const ask = fixture.options!.canUseTool!;
  const a = ask("Write", { path: "a", content: "original" }, opts);
  const b = ask("Write", { content: "original", path: "a" }, opts);
  assert.equal(events.filter((e) => e.kind === "permission_requested").length, 1);
  const changed = ask("Write", { path: "b", content: "original" }, opts);
  for (const wait of [a, b, changed]) assert.equal((await wait)?.behavior, "deny");
  const reports = events.filter((e) => e.kind === "permission_requested");
  assert.equal(reports.length, 2);
  assert.notEqual(reports[0]!.authorization_digest, reports[1]!.authorization_digest);
  assert.throws(() => host.resolvePermission("same", "allow"));
});
