import assert from "node:assert/strict";
import { test, type TestContext } from "node:test";
import type { PermissionUpdate } from "@anthropic-ai/claude-agent-sdk";
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
      { file_path: "file.txt", content: "not for storage" },
      {
        signal: abort.signal,
        toolUseID: "tool-call",
        requestId: `request-${++requestNumber}`,
        agentID,
      },
    );
    const event = events.at(-1)!;
    assert.equal(event.kind, "interaction_requested");
    return { abort, response, id: event.interaction_key };
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
    events.filter((e) => e.kind === "interaction_requested").map((e) => e.blocking),
    [true, false],
  );
  assert.throws(() => host.resolveInteraction(a.id, "always"));
  host.resolveInteraction(b.id, "deny");
  assert.equal((await b.response)?.behavior, "deny");
  assert.equal(aDone, false);
  host.resolveInteraction(a.id, "allow");
  assert.deepEqual(await a.response, {
    behavior: "allow",
    updatedInput: { file_path: "file.txt", content: "not for storage" },
    decisionClassification: "user_temporary",
  });
  assert.throws(() => host.resolveInteraction(a.id, "deny"));
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
  assert.throws(() => host.resolveInteraction(a.id, "allow"));
  host.resolveInteraction(b.id, "allow");
  assert.equal((await b.response)?.behavior, "allow");
  assert.deepEqual(
    events.filter((e) => e.kind === "interaction_settled").map((e) => e.outcome),
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
    host.resolveInteraction(pending.id, "allow");
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
    assert.throws(() => host.resolveInteraction(a.id, "allow"));
    assert.equal(host.status().finished, false);
  });
}

test("background callback after a foreground result is not attached to a new user input", async (t) => {
  const { fixture, events, host, request } = await setup(t);
  fixture.messages.push(result());
  await settle();
  const a = request("background-agent");
  const event = events.at(-1)!;
  assert.equal(event.kind, "interaction_requested");
  assert.equal("input_id" in event, false);
  host.resolveInteraction(a.id, "deny");
  assert.equal((await a.response)?.behavior, "deny");
  assert.equal(events.filter((e) => e.kind === "started").length, 1);
});

test("reordered tool arguments reuse the wait and changed authorization denies both callbacks", async (t) => {
  const { fixture, events, host } = await setup(t);
  const opts = { signal: new AbortController().signal, requestId: "same", toolUseID: "call" };
  const ask = fixture.options!.canUseTool!;
  const a = ask("Write", { path: "a", content: "original" }, opts);
  const b = ask("Write", { content: "original", path: "a" }, opts);
  assert.equal(events.filter((e) => e.kind === "interaction_requested").length, 1);
  const changed = ask("Write", { path: "b", content: "original" }, opts);
  for (const wait of [a, b, changed]) assert.equal((await wait)?.behavior, "deny");
  const reports = events.filter((e) => e.kind === "interaction_requested");
  assert.equal(reports.length, 2);
  assert.notEqual(reports[0]!.authorization_digest, reports[1]!.authorization_digest);
  assert.throws(() => host.resolveInteraction("same", "allow"));
});

test("session approval returns the full SDK suggestions with session destinations", async (t) => {
  const { fixture, events, host } = await setup(t);
  const suggestions: PermissionUpdate[] = [
    { type: "addDirectories", directories: ["/project"], destination: "userSettings" },
    {
      type: "addRules",
      behavior: "allow",
      rules: [{ toolName: "Bash", ruleContent: "npm test:*" }],
      destination: "localSettings",
    },
  ];
  const ask = (suppressAlwaysAllowRule: boolean) =>
    fixture.options!.canUseTool!(
      "Bash",
      { command: "npm test" },
      {
        requestId: "rules",
        toolUseID: "bash",
        signal: new AbortController().signal,
        suggestions,
        suppressAlwaysAllowRule,
      },
    );
  const answer = ask(false);
  const card = events.at(-1)!;
  assert.equal(card.kind, "interaction_requested");
  assert.deepEqual(
    card.options.map((o) => o.key),
    ["allow", "allow_always", "deny"],
  );
  host.resolveInteraction("rules", "allow_always");
  assert.deepEqual(await answer, {
    behavior: "allow",
    updatedInput: { command: "npm test" },
    updatedPermissions: suggestions.map((s) => ({ ...s, destination: "session" })),
    decisionClassification: "user_permanent",
  });
  const suppressed = ask(true);
  assert.throws(() => host.resolveInteraction("rules", "allow_always"));
  host.resolveInteraction("rules", "deny");
  assert.equal((await suppressed)?.behavior, "deny");
});

const question = (i: number) => ({
  question: "Question " + i,
  header: "Topic",
  multiSelect: false,
  options: [
    { label: "First", description: "First choice" },
    { label: "Second", description: "Second choice" },
  ],
});

for (const action of ["answer", "decline", "abort", "multi"] as const) {
  test("grouped SDK questions: " + action, async (t) => {
    const { fixture, host, events } = await setup(t);
    const abort = new AbortController();
    const questions = [question(1), { ...question(2), multiSelect: action === "multi" }];
    const answer = fixture.options!.canUseTool!(
      "AskUserQuestion",
      { questions },
      { signal: abort.signal, requestId: "ask", toolUseID: "call" },
    );
    const cards = events.filter((e) => e.kind === "interaction_requested");
    if (action === "multi") {
      assert.deepEqual(cards, []);
      assert.equal((await answer)?.behavior, "deny");
      return;
    }
    assert.equal(cards.length, 2);
    if (action === "answer") {
      host.resolveInteraction("ask:1", "option_0");
      host.resolveInteraction("ask:0", "option_1");
      assert.deepEqual(await answer, {
        behavior: "allow",
        updatedInput: { questions, answers: { "Question 1": "Second", "Question 2": "First" } },
      });
    } else {
      if (action === "abort") abort.abort();
      else host.resolveInteraction("ask:0", "decline");
      assert.throws(() => host.resolveInteraction("ask:1", "option_0"));
      assert.equal((await answer)?.behavior, "deny");
      assert.equal(
        events.filter((e) => e.kind === "interaction_settled" && e.outcome === "expired").length,
        action === "abort" ? 2 : 1,
      );
    }
  });
}

test("MCP form collects typed choices and omits optional free text", async (t) => {
  const { fixture, host } = await setup(t);
  const ask = fixture.options!.onElicitation!;
  const context = { signal: new AbortController().signal, requestId: "form" };
  const request = {
    serverName: "demo",
    message: "Choose",
    requestedSchema: {
      properties: {
        flag: { type: "boolean" },
        language: { type: "string", oneOf: [{ const: "en", title: "English" }] },
        tone: { type: "string", enum: ["informal"], enumNames: ["Informal"] },
        comment: { type: "string" },
      },
    },
  };
  const answer = ask(request, context);
  for (const [index, choice] of ["option_1", "option_0", "option_0"].entries())
    host.resolveInteraction("elicitation:form:" + index, choice);
  assert.deepEqual(await answer, {
    action: "accept",
    content: { flag: false, language: "en", tone: "informal" },
  });
  const empty = ask({ serverName: "demo", message: "Confirm" }, context);
  host.resolveInteraction("elicitation:form", "accept");
  assert.deepEqual(await empty, { action: "accept", content: {} });
  const declined = ask(request, context);
  host.resolveInteraction("elicitation:form:2", "decline");
  assert.deepEqual(await declined, { action: "decline" });
  assert.throws(() => host.resolveInteraction("elicitation:form:1", "option_0"));
  const cancelled = ask(request, context);
  host.cancelInteraction("elicitation:form:0");
  assert.deepEqual(await cancelled, { action: "cancel" });
  for (const type of ["string", "number", "array"]) {
    const blocked = await ask(
      { ...request, requestedSchema: { required: ["input"], properties: { input: { type } } } },
      context,
    );
    assert.deepEqual(blocked, { action: "decline" });
  }
});

test("replayed grouped forms preserve earlier answers and the original values behind bounded labels", async (t) => {
  const { fixture, host, events } = await setup(t);
  const ask = fixture.options!.onElicitation!;
  const context = { requestId: "replay", signal: new AbortController().signal };
  const longLabel = "选".repeat(205);
  const form = {
    serverName: "catalog",
    title: "📋".repeat(300),
    message: "订".repeat(4100),
    requestedSchema: {
      properties: {
        format: { type: "string", enum: ["markdown"], enumNames: [longLabel] },
        language: { type: "string", enum: ["en", "fr", "es", "de", "it", "nl", "zh", "pt"] },
      },
    },
  };
  const first = ask(form, context);
  const cards = events.filter((e) => e.kind === "interaction_requested");
  assert.equal(cards.length, 2);
  assert.equal(Array.from(cards[0]!.title).length, 256);
  assert.equal(cards[0]!.description.length, 4000);
  assert.equal(cards[0]!.options[0]!.label.length, 200);
  assert.equal(cards[1]!.options.length, 8);
  host.resolveInteraction("elicitation:replay:0", "option_0");
  const replay = ask(form, context);
  assert.equal(events.filter((e) => e.kind === "interaction_requested").length, 2);
  assert.throws(() => host.resolveInteraction("elicitation:replay:0", "option_0"));
  host.resolveInteraction("elicitation:replay:1", "option_6");
  for (const answer of [first, replay])
    assert.deepEqual(await answer, {
      action: "accept",
      content: { format: "markdown", language: "zh" },
    });
});
