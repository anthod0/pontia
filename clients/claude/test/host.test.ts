import assert from "node:assert/strict";
import { test } from "node:test";
import { Host, ControlError } from "../src/host.js";
import type { HostEvent } from "../src/facts.js";
import {
  assistant,
  first,
  init,
  native,
  result,
  sdk,
  second,
  settle,
  stream,
  usage,
} from "./fixture.js";

function setup(resume?: string) {
  const fixture = sdk();
  const events: HostEvent[] = [];
  const host = new Host(
    { session_id: "pontia-session", cwd: "/unused-fixture-workspace", resume },
    (event) => events.push(event),
    fixture.factory,
  );
  return { fixture, events, host };
}
const rejects = (reason: string) => (error: unknown) =>
  error instanceof ControlError && error.reason === reason;

for (const tools of [null, [], ["Read", "Bash", "Glob"]] as const) {
  test(`YOLO applies the requested tool set and does not request approval (${tools})`, async (t) => {
    const fixture = sdk();
    const events: HostEvent[] = [];
    const host = new Host(
      {
        session_id: "policy-session",
        cwd: "/unused-fixture-workspace",
        execution_policy: {
          tools: tools === null ? null : [...tools],
          disabled_tools: [],
          disable_mcp: false,
          disable_skills: false,
          yolo: true,
        },
      },
      (event) => events.push(event),
      fixture.factory,
    );
    t.after(() => host.close());
    host.submit(first, "task");
    const options = fixture.options!;
    assert.equal(options.permissionMode, "bypassPermissions");
    assert.equal(options.allowDangerouslySkipPermissions, true);
    assert.equal(options.sandbox?.enabled, false);
    assert.deepEqual(options.tools, tools === null ? undefined : [...tools]);
    assert.equal(options.strictMcpConfig, undefined);
    assert.equal(options.mcpServers, undefined);
    const context = {
      signal: new AbortController().signal,
      toolUseID: "call",
      requestId: "permission",
    };
    const bash = await options.canUseTool!("Bash", { command: "pwd" }, context);
    assert.equal(bash?.behavior, "allow");
    assert.equal(
      events.some((event) => event.kind === "permission_requested"),
      false,
    );
  });
}

for (const disableMcp of [false, true]) {
  test(`native deny rules and MCP policy reach the SDK on resume (${disableMcp})`, async (t) => {
    const fixture = sdk();
    const host = new Host(
      {
        session_id: "policy-session",
        cwd: "/unused-fixture-workspace",
        resume: native,
        execution_policy: {
          tools: ["Read", "Write", "Bash"],
          disabled_tools: ["Write", "Agent", "Bash(git push *)", "mcp__remote__*"],
          disable_mcp: disableMcp,
          disable_skills: false,
          yolo: true,
        },
      },
      () => {},
      fixture.factory,
    );
    t.after(() => host.close());
    host.submit(first, "task");
    const options = fixture.options!;
    assert.equal(options.resume, native);
    assert.deepEqual(options.tools, ["Read", "Write", "Bash"]);
    assert.deepEqual(options.disallowedTools, [
      "AskUserQuestion",
      "Write",
      "Agent",
      "Bash(git push *)",
      "mcp__remote__*",
      ...(disableMcp ? ["mcp__*"] : []),
    ]);
    assert.equal(options.strictMcpConfig, disableMcp ? true : undefined);
    assert.deepEqual(options.mcpServers, disableMcp ? {} : undefined);
  });
}

for (const disableSkills of [false, true]) {
  for (const resume of [undefined, native]) {
    test(`skill loading policy reaches the SDK (${disableSkills}, resume=${!!resume})`, (t) => {
      const fixture = sdk();
      const host = new Host(
        {
          session_id: "policy-session",
          cwd: "/unused-fixture-workspace",
          resume,
          execution_policy: {
            tools: null,
            disabled_tools: [],
            disable_mcp: false,
            disable_skills: disableSkills,
            yolo: false,
          },
        },
        () => {},
        fixture.factory,
      );
      t.after(() => host.close());
      host.submit(first, "task");
      assert.deepEqual(
        fixture.options!.extraArgs,
        disableSkills ? { "disable-slash-commands": null } : undefined,
      );
      assert.deepEqual(fixture.options!.settingSources, ["user", "project", "local"]);
    });
  }
}

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
    ["ready", "model"],
  );
  // A notification-started foreground round can subsequently consume the user input.
  fixture.messages.push(assistant(first));
  fixture.messages.push(result(first));
  await settle();
  assert.deepEqual(
    events.map((e) => e.kind),
    ["ready", "model", "started", "text", "finished"],
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

for (const resume of [undefined, native]) {
  test(`appends fixed instructions while retaining Claude Code preset (resume=${!!resume})`, async (t) => {
    const fixture = sdk();
    const host = new Host(
      {
        session_id: "pontia-session",
        cwd: "/unused-fixture-workspace",
        resume,
        system_prompt: "creation marker",
      },
      () => {},
      fixture.factory,
    );
    t.after(() => host.close());
    host.submit(first, "task");
    await fixture.next();
    assert.deepEqual(fixture.options?.systemPrompt, {
      type: "preset",
      preset: "claude_code",
      append: "creation marker",
    });
    assert.deepEqual(fixture.options?.settingSources, ["user", "project", "local"]);
    assert.equal(fixture.options?.resume, resume);
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    fixture.messages.push(result());
    await settle();
    host.submit(second, "follow-up");
    assert.equal(fixture.calls, 1);
  });
}

for (const mode of ["combined", "after text", "after tool use"] as const) {
  test(`steering stays in the active task with native results ${mode}`, async (t) => {
    const { fixture, events, host } = setup();
    t.after(() => host.close());
    host.submit(first, "original");
    await fixture.next();
    fixture.messages.push(init());
    fixture.messages.push(assistant());
    await settle();
    host.steer(first, second, "change direction");
    const steering = (await fixture.next()).value;
    assert.equal(steering.uuid, second);
    assert.equal(steering.priority, "now");
    assert.equal(steering.message.content, "change direction");
    assert.equal(fixture.interrupts, 0);
    if (mode !== "combined") {
      fixture.messages.push(
        result(first, {
          subtype: "success",
          is_error: false,
          stop_reason: mode === "after tool use" ? "tool_use" : null,
          terminal_reason: "aborted_streaming",
        }),
      );
      await settle();
      assert.equal(events.filter((e) => e.kind === "finished").length, 0);
      assert.equal(host.isIdle(), false);
      assert.throws(() => host.submit(native, "queued"), rejects("input_busy"));
    }
    fixture.messages.push(assistant(second, [{ type: "text", text: "redirected" }], "steered"));
    fixture.messages.push(
      result(second, mode === "combined" ? { user_message_uuids: [first, second] } : {}),
    );
    await settle();
    assert.equal(host.status().unavailable, null);
    assert.deepEqual(
      events.filter((e) => e.kind === "started").map((e) => e.input_id),
      [first],
    );
    assert.deepEqual(
      events.filter((e) => e.kind === "finished").map((e) => [e.input_id, e.outcome]),
      [[first, "completed"]],
    );
    assert.ok(
      events.some((e) => e.kind === "text" && e.input_id === first && e.delta === "redirected"),
    );
    const consumed = events.findIndex((e) => e.kind === "input");
    assert.deepEqual(events[consumed], {
      kind: "input",
      input_id: first,
      item_id: second,
      text: "change direction",
    });
    assert.equal(events.filter((e) => e.kind === "input").length, 1);
    assert.ok(consumed < events.findIndex((e) => e.kind === "text" && e.delta === "redirected"));
    assert.equal(host.isIdle(), true);
    host.submit(native, "next task");
    assert.equal((await fixture.next()).value.uuid, native);
  });
}

test("steering rejects stale, finished and disconnected targets without consuming the message ID", async (t) => {
  const { fixture, host } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  await fixture.next();
  assert.throws(() => host.steer(first, second, "too early"), rejects("no_matching_execution"));
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  await settle();
  assert.throws(() => host.steer(second, second, "wrong target"), rejects("no_matching_execution"));
  host.controlLost();
  assert.throws(() => host.steer(first, second, "disconnected"), rejects("execution_unavailable"));
  host.controlRestored();
  fixture.messages.push(result());
  await settle();
  assert.throws(() => host.steer(first, second, "too late"), rejects("no_matching_execution"));
  host.submit(second, "next task");
  assert.equal((await fixture.next()).value.uuid, second);
});

test("multiple steering messages are delivered in order and all must resolve before the task ends", async (t) => {
  const { fixture, host, events } = setup();
  t.after(() => host.close());
  host.submit(first, "task");
  await fixture.next();
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  await settle();
  host.steer(first, second, "first correction");
  host.steer(first, native, "second correction");
  assert.throws(() => host.steer(first, second, "duplicate"), rejects("input_already_submitted"));
  assert.equal((await fixture.next()).value.uuid, second);
  assert.equal((await fixture.next()).value.uuid, native);
  fixture.messages.push(result(first));
  fixture.messages.push(assistant(second));
  fixture.messages.push(result(second));
  await settle();
  assert.equal(host.status().finished, false);
  fixture.messages.push(assistant(native));
  fixture.messages.push(result(native));
  await settle();
  assert.equal(host.status().unavailable, null);
  assert.equal(events.filter((e) => e.kind === "finished").length, 1);
});

function apiReply(input: string, inputTokens: number) {
  const reply = assistant(input, undefined, "message-" + inputTokens);
  return {
    ...reply,
    message: {
      ...reply.message,
      model: "sonnet",
      usage: {
        input_tokens: inputTokens,
        cache_creation_input_tokens: 233,
        cache_read_input_tokens: 477,
        output_tokens: 73,
      },
    },
  };
}

const compactBoundary = {
  type: "system",
  subtype: "compact_boundary",
  session_id: native,
  uuid: "usage-compact",
  compact_metadata: { trigger: "auto", pre_tokens: 14710, post_tokens: 519 },
};

test("main API replies replace initial and post-compaction estimates with per-request usage", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  let snapshot = usage(500);
  fixture.onContextUsage(async () => snapshot);
  const observations = () =>
    events.filter((e) => e.kind === "context_usage").map((e) => e.context_usage);
  const end = (input: string) =>
    result(input, {
      usage: { input_tokens: 9000000 },
      modelUsage: { haiku: { contextWindow: 100000 }, sonnet: { contextWindow: 1000000 } },
    });
  host.submit(first, "first task");
  fixture.messages.push(init());
  await settle();
  fixture.messages.push(apiReply(first, 13000));
  await settle();
  assert.equal(observations().at(-1)?.used_tokens, 13710);
  fixture.messages.push(end(first));
  await settle();
  host.submit(second, "second task");
  fixture.messages.push(init());
  fixture.messages.push(apiReply(second, 14000));
  fixture.messages.push(apiReply(second, 14000));
  fixture.messages.push({ ...apiReply(second, 80000), parent_tool_use_id: "subagent" });
  fixture.messages.push({
    ...stream({ type: "message_start", message: apiReply(second, 80000).message }),
    parent_tool_use_id: "subagent",
  });
  fixture.messages.push({
    ...stream({ type: "message_delta", usage: { output_tokens: 10 } }),
    parent_tool_use_id: "subagent",
  });
  fixture.messages.push(end(second));
  await settle();
  assert.deepEqual(
    observations().map((u) => [u.used_tokens, u.confidence]),
    [
      [500, "estimated"],
      [13710, "exact"],
      [14710, "exact"],
    ],
  );
  assert.deepEqual(observations().at(-1), {
    used_tokens: 14710,
    max_tokens: 200000,
    remaining_tokens: 185290,
    usage_ratio: 0.07355,
    input_tokens: 14000,
    cache_tokens: 710,
    output_tokens: 73,
    confidence: "exact",
  });
  snapshot = usage(622);
  fixture.messages.push(compactBoundary);
  await settle();
  assert.deepEqual(observations().at(-1), {
    used_tokens: 622,
    max_tokens: 200000,
    remaining_tokens: 199378,
    usage_ratio: 0.00311,
    confidence: "estimated",
  });
});

test("resume uses the local estimate until the next main API reply", async (t) => {
  const { fixture, events, host } = setup(native);
  t.after(() => host.close());
  fixture.onContextUsage(async () => usage(664));
  host.submit(first, "resumed task");
  fixture.messages.push(init());
  await settle();
  fixture.messages.push(apiReply(first, 16000));
  fixture.messages.push(result());
  await settle();
  assert.deepEqual(
    events
      .filter((e) => e.kind === "context_usage")
      .map((e) => [e.context_usage.used_tokens, e.context_usage.confidence]),
    [
      [664, "estimated"],
      [16710, "exact"],
    ],
  );
});

test("a delayed window lookup publishes only the latest usage observation", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  let resolve!: (snapshot: ReturnType<typeof usage>) => void;
  fixture.onContextUsage(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(apiReply(first, 13000));
  await settle();
  resolve(usage(100));
  await settle();

  // This exact reply is waiting on an earlier lookup when compaction supersedes it.
  fixture.messages.push(compactBoundary);
  fixture.messages.push(apiReply(first, 14000));
  await settle();
  const older = resolve;
  fixture.messages.push({ ...compactBoundary, uuid: "newer-boundary" });
  await settle();
  resolve(usage(520));
  await settle();
  older(usage(600));
  await settle();
  assert.deepEqual(
    events
      .filter((e) => e.kind === "context_usage")
      .map((e) => [e.context_usage.used_tokens, e.context_usage.confidence]),
    [
      [13710, "exact"],
      [520, "estimated"],
    ],
  );
});

test("API usage recovers after a failed initial window lookup", async (t) => {
  const { fixture, events, host } = setup();
  t.after(() => host.close());
  fixture.onContextUsage(async () => {
    throw new Error("private SDK error");
  });
  host.submit(first, "task");
  fixture.messages.push(init());
  await settle();
  fixture.onContextUsage(async () => usage(100, 10000));
  fixture.messages.push(apiReply(first, 13000));
  fixture.messages.push(result());
  await settle();
  assert.equal(host.status().finished, true);
  assert.deepEqual(
    events.filter((e) => e.kind === "context_usage").map((e) => e.context_usage),
    [
      {
        used_tokens: 13710,
        max_tokens: 10000,
        remaining_tokens: 0,
        usage_ratio: 1,
        input_tokens: 13000,
        output_tokens: 73,
        cache_tokens: 710,
        confidence: "exact",
      },
    ],
  );
});

test("slow context queries do not delay output or completion and cannot publish after close", async () => {
  const { fixture, events, host } = setup();
  let resolve!: (value: ReturnType<typeof usage>) => void;
  fixture.onContextUsage(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  host.submit(first, "task");
  fixture.messages.push(init());
  fixture.messages.push(assistant());
  fixture.messages.push(result());
  await settle();
  assert.equal(host.status().finished, true);
  assert.equal(events.filter((e) => e.kind === "text").length, 1);
  await host.close();
  resolve(usage(100));
  await settle();
  assert.equal(events.filter((e) => e.kind === "context_usage").length, 0);
});
