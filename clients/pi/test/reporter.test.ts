import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test, vi } from "vitest";
import { buildSessionContextUsageUpdatedEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, buildTurnStartedEvent, contextUsageFromPiContext, contextUsageFromPiEvent } from "../src/events.js";
import { EventReporter } from "../src/reporter.js";
import type { TurnContext } from "../src/context.js";

const tmpDirs: string[] = [];

afterEach(async () => {
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
});

async function tempLogFile() {
  const dir = await mkdtemp(join(tmpdir(), "pontia-pi-reporter-"));
  tmpDirs.push(dir);
  return join(dir, "pi-hook.log");
}

const context: TurnContext = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
  internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
};

describe("event builders", () => {
  test("builds turn.started payload shape", () => {
    const event = buildTurnStartedEvent(context);

    expect(event.event_id).toMatch(/^evt_/);
    expect(event).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      source: "agent_adapter",
      client_type: "pi",
      type: "turn.started",
      seq: null,
      payload: { runtime_instance_id: "rtinst_1", input: {} },
    });
    expect(new Date(event.time).toISOString()).toBe(event.time);
  });

  test("builds turn.output payload shape", () => {
    const event = buildTurnOutputEvent(context, "hello");

    expect(event.event_id).toMatch(/^evt_/);
    expect(event).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      source: "agent_adapter",
      client_type: "pi",
      type: "turn.output",
      seq: null,
      payload: { output: { summary: "hello" } },
    });
    expect(new Date(event.time).toISOString()).toBe(event.time);
  });

  test("builds turn.completed payload shape", () => {
    expect(buildTurnCompletedEvent(context)).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      source: "agent_adapter",
      client_type: "pi",
      type: "turn.completed",
      seq: null,
      payload: {},
    });
  });

  test("builds turn.failed payload shape", () => {
    expect(buildTurnFailedEvent(context, "boom")).toMatchObject({
      type: "turn.failed",
      payload: { failure: { message: "boom" } },
    });
  });

  test("builds session.context_usage_updated payload shape", () => {
    const event = buildSessionContextUsageUpdatedEvent(context, {
      used_tokens: 42,
      max_tokens: 100,
      remaining_tokens: 58,
      usage_ratio: 0.42,
      input_tokens: 40,
      output_tokens: 2,
      cache_tokens: null,
      model: "example-model",
      confidence: "exact",
    });

    expect(event).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      source: "agent_client",
      client_type: "pi",
      type: "session.context_usage_updated",
      seq: null,
      payload: {
        context_usage: {
          used_tokens: 42,
          max_tokens: 100,
          remaining_tokens: 58,
          usage_ratio: 0.42,
          input_tokens: 40,
          output_tokens: 2,
          cache_tokens: null,
          model: "example-model",
          confidence: "exact",
        },
      },
    });
  });

  test("does not extract context usage from unsupported pi hook payloads", () => {
    expect(contextUsageFromPiEvent({ assistantMessageEvent: { text_delta: "hello" } })).toBeUndefined();
    expect(contextUsageFromPiEvent({ messages: [] })).toBeUndefined();
  });

  test("extracts context usage only when a hook payload exposes a valid context_usage object", () => {
    expect(
      contextUsageFromPiEvent({
        context_usage: {
          used_tokens: 1,
          max_tokens: 4,
          usage_ratio: 0.25,
          confidence: "estimated",
        },
      }),
    ).toEqual({
      used_tokens: 1,
      max_tokens: 4,
      remaining_tokens: null,
      usage_ratio: 0.25,
      input_tokens: null,
      output_tokens: null,
      cache_tokens: null,
      model: null,
      confidence: "estimated",
    });
  });

  test("extracts estimated context usage from pi extension context", () => {
    expect(
      contextUsageFromPiContext({
        model: { id: "gpt-5.5" },
        getContextUsage: () => ({ tokens: 6_037, contextWindow: 128_000, percent: 4.716 }),
      }),
    ).toEqual({
      used_tokens: 6037,
      max_tokens: 128000,
      remaining_tokens: 121963,
      usage_ratio: 0.04716,
      input_tokens: null,
      output_tokens: null,
      cache_tokens: null,
      model: "gpt-5.5",
      confidence: "estimated",
    });
  });

  test("extracts estimated context usage from pi message usage fallback", () => {
    expect(
      contextUsageFromPiEvent({
        message: {
          role: "assistant",
          model: "gpt-5.5",
          usage: { input: 386, output: 19, cacheRead: 5632, totalTokens: 6037 },
        },
      }),
    ).toEqual({
      used_tokens: 6037,
      max_tokens: null,
      remaining_tokens: null,
      usage_ratio: null,
      input_tokens: 386,
      output_tokens: 19,
      cache_tokens: 5632,
      model: "gpt-5.5",
      confidence: "estimated",
    });
  });
});

describe("EventReporter", () => {
  test("posts event JSON to the Internal Event API", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ accepted: true }), { status: 202 }));
    const reporter = new EventReporter({ fetch, logFile: await tempLogFile() });
    const event = buildTurnCompletedEvent(context);

    await reporter.report(context, event);

    expect(fetch).toHaveBeenCalledWith(context.internalEventUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(event),
    });
  });

  test("logs non-2xx POST failures and returns false", async () => {
    const logFile = await tempLogFile();
    const fetch = vi.fn(async () => new Response("nope", { status: 500, statusText: "Server Error" }));
    const reporter = new EventReporter({ fetch, logFile });

    const ok = await reporter.report(context, buildTurnCompletedEvent(context));

    expect(ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("internal_event_post_failed");
    expect(log).toContain("500");
  });

  test("logs thrown POST errors and returns false", async () => {
    const logFile = await tempLogFile();
    const fetch = vi.fn(async () => {
      throw new Error("network down");
    });
    const reporter = new EventReporter({ fetch, logFile });

    const ok = await reporter.report(context, buildTurnCompletedEvent(context));

    expect(ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("internal_event_post_exception");
    expect(log).toContain("network down");
  });
});
