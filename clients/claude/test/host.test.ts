import assert from "node:assert/strict";
import { test } from "node:test";
import { Host, ControlError } from "../src/host.js";
import type { HostEvent } from "../src/facts.js";
import { assistant, first, init, native, result, sdk, second, settle, stream } from "./fixture.js";

function setup(resume?: string) {
  const fixture = sdk();
  const events: HostEvent[] = [];
  const host = new Host(
    { cwd: "/unused-fixture-workspace", resume },
    (event) => events.push(event),
    fixture.factory,
  );
  return { fixture, events, host };
}
const rejects = (reason: string) => (error: unknown) =>
  error instanceof ControlError && error.reason === reason;

test("authentication retries are visible without fabricating a Turn and clear when SDK execution recovers", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  await fixture.next();
  fixture.messages.push(init());
  fixture.messages.push({
    type: "system",
    subtype: "api_retry",
    session_id: native,
    error: "authentication_failed",
    error_status: 401,
  });
  await settle();
  assert.deepEqual(
    events.filter((e) => e.kind === "retry"),
    [{ kind: "retry", reason: "authentication_failed_retrying" }],
  );
  assert.equal(events.filter((e) => e.kind === "started" || e.kind === "finished").length, 0);
  assert.throws(() => host.submit(second, "next"), rejects("input_busy"));
  fixture.messages.push(assistant());
  fixture.messages.push(result());
  await settle();
  assert.deepEqual(events.filter((e) => e.kind === "retry").at(-1), {
    kind: "retry",
    reason: null,
  });
  assert.equal(events.filter((e) => e.kind === "finished").length, 1);
  host.submit(second, "next");
});

test("starts with the actual first input and admits only one unresolved user task", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  assert.equal(fixture.calls, 0);
  host.submit(first, "first task");
  assert.equal(fixture.calls, 1);
  assert.deepEqual((await fixture.next()).value.message, { role: "user", content: "first task" });
  assert.equal(events.length, 0);
  assert.throws(() => host.submit(second, "queued"), rejects("input_busy"));
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  await settle();
  assert.equal(events.filter((e) => e.kind === "started").length, 1);
  assert.throws(() => host.submit(second, "queued"), rejects("input_busy"));
  fixture.messages.push(result());
  await settle();
  host.submit(second, "second task");
  assert.equal((await fixture.next()).value.uuid, second);
  assert.equal(fixture.calls, 1);
  assert.throws(() => host.submit(first, "replay"), rejects("input_already_submitted"));
});

test("automatic and subagent messages do not start or complete the next input", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push({ ...assistant(first), parent_tool_use_id: "parent" });
  fixture.messages.push({ ...assistant(), user_message_uuid: undefined });
  fixture.messages.push({ ...result(), user_message_uuid: undefined });
  await settle();
  assert.deepEqual(
    events.map((e) => e.kind),
    ["ready"],
  );
  // A notification-started foreground round can subsequently consume the user input.
  fixture.messages.push(assistant(first));
  fixture.messages.push(result(first));
  await settle();
  assert.deepEqual(
    events.map((e) => e.kind),
    ["ready", "started", "text", "finished"],
  );
});

test("streamed text and full assistant messages render once; tool generation is not execution completion", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(stream({ type: "message_start", message: { id: "reply" } }, first));
  fixture.messages.push(
    stream({ type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }),
  );
  fixture.messages.push(
    stream({
      type: "content_block_delta",
      index: 0,
      delta: { type: "text_delta", text: "answer" },
    }),
  );
  fixture.messages.push(stream({ type: "message_stop" }));
  fixture.messages.push(
    assistant(first, [
      { type: "text", text: "answer" },
      { type: "tool_use", id: "call", name: "Read", input: { file_path: "file" } },
    ]),
  );
  fixture.messages.push(assistant(first));
  await settle();
  assert.deepEqual(
    events.filter((e) => e.kind === "text").map((e) => e.delta),
    ["answer"],
  );
  assert.equal(events.filter((e) => e.kind === "tool").length, 1);
  assert.equal(events.filter((e) => e.kind === "tool_result" || e.kind === "finished").length, 0);
  fixture.messages.push({
    type: "user",
    session_id: native,
    parent_tool_use_id: null,
    message: {
      content: [{ type: "tool_result", tool_use_id: "call", is_error: true, content: "not found" }],
    },
  });
  await settle();
  assert.equal(events.filter((e) => e.kind === "tool_result" && e.failed).length, 1);
  assert.equal(events.filter((e) => e.kind === "finished").length, 0);
  fixture.messages.push(result());
  await settle();
  assert.equal(events.filter((e) => e.kind === "finished" && e.outcome === "completed").length, 1);
});

test("separate assistant block envelopes sharing an API message preserve text and tool results", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(
    assistant(first, [{ type: "thinking", thinking: "reasoning" }], "shared", "thinking"),
  );
  fixture.messages.push(assistant(first, [{ type: "text", text: "one" }], "shared", "text-one"));
  fixture.messages.push(assistant(first, [{ type: "text", text: "two" }], "shared", "text-two"));
  const tool = assistant(
    first,
    [{ type: "tool_use", id: "call", name: "Read", input: {} }],
    "shared",
    "tool",
  );
  fixture.messages.push(tool);
  fixture.messages.push(tool);
  fixture.messages.push({
    type: "user",
    session_id: native,
    parent_tool_use_id: null,
    message: { content: [{ type: "tool_result", tool_use_id: "call", content: "file contents" }] },
  });
  await settle();
  const text = events.filter((e) => e.kind === "text");
  assert.deepEqual(
    text.map((e) => e.delta),
    ["one", "two"],
  );
  assert.notEqual(text[0]!.item_id, text[1]!.item_id);
  assert.equal(events.filter((e) => e.kind === "tool").length, 1);
  assert.deepEqual(
    events.filter((e) => e.kind === "tool_result").map((e) => e.content),
    ["file contents"],
  );
});

test("a correlated API error assistant can start and fail a task without a text stream", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(assistant(first, [{ type: "text", text: "API unavailable" }]));
  fixture.messages.push(
    result(first, { is_error: true, stop_reason: "stop_sequence", terminal_reason: "api_error" }),
  );
  await settle();
  assert.deepEqual(
    events.filter((e) => e.kind === "started" || e.kind === "finished").map((e) => e.kind),
    ["started", "finished"],
  );
  assert.equal(events.filter((e) => e.kind === "finished" && e.outcome === "failed").length, 1);
});

for (const [label, reply, terminal] of [
  ["result without a start", undefined, result()],
  ["unknown terminal reason", assistant(), result(first, { terminal_reason: "future_reason" })],
  ["conflicting UUIDs", assistant(), result(first, { user_message_uuids: [first, second] })],
  [
    "conflicting scalar and array UUID",
    assistant(),
    result(second, { user_message_uuids: [first] }),
  ],
  ["wrong input UUID", assistant(second), result(second)],
] as const) {
  test(`${label} leaves execution unavailable, without guessing a terminal fact`, async (t) => {
    const { fixture, events, host } = setup();
    t.after(() => host.close());
    host.submit(first, "task");
    fixture.messages.push(init());
    if (reply) fixture.messages.push(reply);
    fixture.messages.push(terminal);
    await settle();
    assert.equal(events.filter((e) => e.kind === "finished").length, 0);
    assert.ok(host.status().unavailable);
    if (label === "unknown terminal reason") {
      assert.deepEqual(
        events.find((e) => e.kind === "unavailable"),
        {
          kind: "unavailable",
          reason: "unsupported_result",
          result: {
            subtype: "success",
            is_error: false,
            stop_reason: "end_turn",
            terminal_reason: "future_reason",
          },
        },
      );
    }
    assert.throws(() => host.submit(second, "next"), rejects("execution_unavailable"));
  });
}

for (const [terminal, expected] of [
  [result(), "completed"],
  [
    result(first, {
      subtype: "error_during_execution",
      is_error: true,
      stop_reason: null,
      terminal_reason: "aborted_streaming",
    }),
    "interrupted",
  ],
  [
    result(first, {
      subtype: "error_during_execution",
      is_error: true,
      stop_reason: "tool_use",
      terminal_reason: "aborted_tools",
    }),
    "interrupted",
  ],
] as const) {
  test(`interrupt acknowledgement does not decide the actual ${expected} result (${terminal.terminal_reason})`, async (t) => {
    const { fixture, events, host } = setup();
    t.after(() => host.close());
    host.submit(first, "task");
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    await settle();
    await assert.rejects(host.interrupt(second), rejects("no_matching_execution"));
    await host.interrupt(first);
    assert.equal(fixture.interrupts, 1);
    assert.equal(events.filter((e) => e.kind === "finished").length, 0);
    assert.throws(() => host.submit(second, "next"), rejects("input_busy"));
    fixture.messages.push(terminal);
    await settle();
    assert.deepEqual(
      events.filter((e) => e.kind === "finished").map((e) => e.outcome),
      [expected],
    );
  });
}

test("explicit close waits for SDK reader end and never manufactures a Turn result", async () => {
  const { fixture, events, host } = setup();
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  await settle();
  await host.close();
  assert.equal(host.status().closed, true);
  assert.equal(events.filter((e) => e.kind === "closed").length, 1);
  assert.equal(events.filter((e) => e.kind === "finished").length, 0);
  await host.close();
  assert.equal(events.filter((e) => e.kind === "closed").length, 1);
});

test("SDK reader failure cannot confirm close or a task result", async () => {
  const { fixture, events, host } = setup();
  host.submit(first, "task");
  fixture.messages.failure = new Error("fixture execution lost");
  fixture.messages.end();
  await settle();
  await assert.rejects(host.close(), rejects("close_unconfirmed"));
  assert.equal(host.status().closed, false);
  assert.equal(events.filter((e) => e.kind === "finished" || e.kind === "closed").length, 0);
});

test("resume always selects the bound native identity and rejects a replacement", async (t) => {
  const { fixture, events, host } = setup(native);
  t.after(() => host.close());
  host.submit(first, "resume task");
  assert.equal(fixture.options?.resume, native);
  assert.equal(fixture.options?.continue, undefined);
  assert.equal((await fixture.next()).value.session_id, native);
  fixture.messages.push(init("different-native"));
  fixture.messages.push(assistant());
  await settle();
  assert.equal(events.filter((e) => e.kind === "ready" || e.kind === "started").length, 0);
  assert.equal(host.status().unavailable, "native_identity_mismatch");
});

test("unsupported questions and elicitation decline without auto-approval or a new input", async (t) => {
  const { fixture, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  const options = fixture.options!;
  const permission = await options.canUseTool!(
    "AskUserQuestion",
    {},
    { signal: new AbortController().signal, toolUseID: "call", requestId: "permission" },
  );
  assert.equal(permission?.behavior, "deny");
  const elicitation = await options.onElicitation!(
    { serverName: "test", message: "input", mode: "form" },
    { signal: new AbortController().signal, requestId: "request" },
  );
  assert.equal(elicitation?.action, "decline");
  assert.deepEqual(options.disallowedTools, ["AskUserQuestion"]);
  assert.equal(options.permissionMode, "default");
  assert.equal(options.tools, undefined);
  assert.deepEqual(options.settingSources, ["user", "project", "local"]);
});

test("control loss does not close SDK or release an uncertain execution for another input", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  await settle();
  host.controlLost();
  assert.equal(host.status().closed, false);
  assert.equal(events.filter((e) => e.kind === "finished").length, 0);
  await host.interrupt(first);
  fixture.messages.push(result());
  await settle();
  assert.throws(() => host.submit(second, "next"), rejects("execution_unavailable"));
});
