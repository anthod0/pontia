import assert from "node:assert/strict";
import { test } from "node:test";
import { Host, ControlError } from "../src/host.js";
import type { HostEvent } from "../src/facts.js";
import { assistant, first, init, native, result, sdk, second, settle } from "./fixture.js";

async function ready(t: { after: (fn: () => Promise<void>) => void }) {
  const fixture = sdk();
  const events: HostEvent[] = [];
  const host = new Host(
    { session_id: "session", cwd: "/unused" },
    (e) => events.push(e),
    fixture.factory,
  );
  t.after(() => host.close());
  host.submit(first, "task");
  await fixture.next();
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  fixture.messages.push(result());
  await settle();
  return { fixture, events, host, facts: () => events.filter((e) => e.kind === "compaction") };
}
const status = (extra: object = {}) => ({
  type: "system",
  subtype: "status",
  session_id: native,
  status: "compacting",
  ...extra,
});
const boundary = (trigger = "manual", uuid = "checkpoint") => ({
  type: "system",
  subtype: "compact_boundary",
  session_id: native,
  uuid,
  compact_metadata: { trigger, pre_tokens: 20000, post_tokens: 2000 },
});

test("manual compaction waits for its command result, rejects busy controls, and creates no Turn", async (t) => {
  const f = await ready(t);
  f.host.compact("compact-api");
  const command = (await f.fixture.next()).value!;
  assert.equal(command.message.content, "/compact");
  assert.equal(command.session_id, native);
  assert.equal(f.host.isIdle(), false);
  assert.throws(() => f.host.submit(second, "busy"), ControlError);
  assert.throws(() => f.host.compact("second"), ControlError);
  f.fixture.messages.push(status());
  f.fixture.messages.push(status());
  f.fixture.messages.push(status({ status: null, compact_result: "success" }));
  f.fixture.messages.push(boundary());
  f.fixture.messages.push(boundary());
  await settle();
  assert.equal(f.host.isIdle(), false);
  assert.deepEqual(
    f.facts().map((e) => e.phase),
    ["started"],
  );
  f.fixture.messages.push(result(command.uuid, { terminal_reason: undefined, result: "" }));
  await settle();
  assert.equal(f.host.isIdle(), true);
  assert.deepEqual(
    f.facts().map((e) => e.phase),
    ["started", "completed"],
  );
  assert.deepEqual(f.facts()[1]!.data, {
    operation_id: "compact-api",
    trigger: "manual",
    native_compaction_id: "checkpoint",
    tokens_before: 20000,
    tokens_after: 2000,
  });
  assert.equal(f.events.filter((e) => e.kind === "started").length, 1);
  assert.equal(f.events.filter((e) => e.kind === "finished").length, 1);
  assert.equal(f.host.status().unavailable, null);
  f.host.submit(second, "next task");
  assert.equal((await f.fixture.next()).value!.message.content, "next task");
});

for (const failure of ["empty", "error", "aborted"] as const) {
  test(`manual compaction ${failure} releases busy state without reporting completion`, async (t) => {
    const f = await ready(t);
    f.host.compact("operation");
    const command = (await f.fixture.next()).value!;
    if (failure === "error") {
      f.fixture.messages.push(status());
      f.fixture.messages.push(
        status({
          status: null,
          compact_result: "failed",
          compact_error: "private provider diagnostics",
        }),
      );
      await settle();
      assert.equal(f.host.isIdle(), false);
    }
    f.fixture.messages.push(
      result(command.uuid, {
        is_error: failure !== "empty",
        terminal_reason: failure === "aborted" ? "aborted_streaming" : undefined,
        result: "Not enough messages to compact.",
      }),
    );
    await settle();
    assert.deepEqual(
      f.facts().map((e) => e.phase),
      ["started", "failed"],
    );
    assert.equal(f.facts()[1]!.data.aborted, failure === "aborted");
    assert.equal(f.host.isIdle(), true);
    assert.equal(f.host.status().unavailable, null);
    assert.ok(!JSON.stringify(f.events).includes("private provider diagnostics"));
    f.host.compact("retry");
    assert.equal((await f.fixture.next()).value!.message.content, "/compact");
  });
}

test("automatic compaction reports a paired lifecycle and preserves the active task", async (t) => {
  const f = await ready(t);
  f.host.submit(second, "active");
  await f.fixture.next();
  f.fixture.messages.push(assistant(second));
  f.fixture.messages.push(status());
  f.fixture.messages.push(boundary("auto"));
  f.fixture.messages.push(boundary("auto"));
  f.fixture.messages.push(result(second));
  await settle();
  assert.deepEqual(
    f.facts().map((e) => e.phase),
    ["started", "completed"],
  );
  assert.equal(f.facts()[0]!.data.trigger, "unknown");
  assert.equal(f.facts()[0]!.data.operation_id, f.facts()[1]!.data.operation_id);
  assert.equal(f.events.filter((e) => e.kind === "finished").length, 2);
  assert.equal(f.host.isIdle(), true);
});

test("automatic failure permits another compaction and does not finish a Turn", async (t) => {
  const f = await ready(t);
  for (let i = 0; i < 2; i++) {
    f.fixture.messages.push(status());
    f.fixture.messages.push(status({ status: null, compact_result: "failed" }));
    await settle();
  }
  assert.deepEqual(
    f.facts().map((e) => e.phase),
    ["started", "failed", "started", "failed"],
  );
  assert.notEqual(f.facts()[0]!.data.operation_id, f.facts()[2]!.data.operation_id);
  assert.equal(f.host.isIdle(), true);
});

test("foreign boundaries fail closed and never complete compaction", async (t) => {
  const f = await ready(t);
  f.host.compact("operation");
  f.fixture.messages.push({ ...boundary(), session_id: "other" });
  await settle();
  assert.equal(f.host.status().unavailable, "native_identity_mismatch");
  assert.deepEqual(
    f.facts().map((e) => e.phase),
    ["started", "failed"],
  );
  assert.equal(f.facts()[1]!.data.aborted, false);
});

for (const failure of ["ended", "reader", "association"] as const) {
  test(`SDK ${failure} failure ends a pending compaction`, async (t) => {
    const f = await ready({
      after(close) {
        t.after(async () => {
          if (failure === "association") await close();
          else await assert.rejects(close, /close_unconfirmed/);
        });
      },
    });
    f.host.compact("operation");
    await f.fixture.next();
    f.fixture.messages.push(status());
    if (failure === "association") f.fixture.messages.push(result(second));
    else {
      if (failure === "reader") f.fixture.messages.failure = new Error("reader failed");
      f.fixture.messages.end();
    }
    await settle();
    assert.deepEqual(
      f.facts().map((e) => e.phase),
      ["started", "failed"],
    );
    assert.equal(f.facts()[1]!.data.aborted, false);
    assert.ok(f.host.status().unavailable);
  });
}
