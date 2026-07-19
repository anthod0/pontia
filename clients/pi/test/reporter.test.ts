import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test, vi } from "vitest";
import { buildSessionContextUsageUpdatedEvent, buildTurnCompletedEvent, buildTurnFailedEvent, buildTurnOutputEvent, buildTurnStartedEvent, contextUsageFromPiContext, contextUsageFromPiEvent, contextUsageFromPiHook } from "../src/events.js";
import { EventReporter } from "../src/reporter.js";
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

const context = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
  internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
} as const;

describe("event builders", () => {
  test("builds a turn.started observation without Pontia-owned identity", () => {
    const event = buildTurnStartedEvent({ ...context, input: "from web", inboxMessageId: "msg_1" });

    expect(event).toEqual({
      session_id: "sess_1",
      type: "turn.started",
      data: {
        runtime_instance_id: "rtinst_1",
        input_summary: "from web",
        previous_leaf_id: null,
        inbox_message_id: "msg_1",
      },
    });
    for (const field of ["event_id", "turn_id", "source", "client_type", "time"]) {
      expect(event).not.toHaveProperty(field);
    }
  });

  test("builds turn output and terminal observations using the canonical turn id", () => {
    expect(buildTurnOutputEvent(context, "hello")).toEqual({
      session_id: "sess_1",
      turn_id: "turn_1",
      type: "turn.output",
      data: { output_summary: "hello" },
    });
    expect(buildTurnCompletedEvent(context)).toMatchObject({
      turn_id: "turn_1",
      type: "turn.completed",
      data: { runtime_instance_id: "rtinst_1", terminal_leaf_id: null },
    });
    expect(buildTurnFailedEvent(context, "boom")).toMatchObject({
      type: "turn.failed",
      data: { failure_message: "boom" },
    });
  });

  test("truncates turn.output summaries to 200 Unicode characters", () => {
    const event = buildTurnOutputEvent(context, "界".repeat(201));
    expect(event.data).toEqual({ output_summary: "界".repeat(200) });
  });

  test("builds session.context_usage_updated observation without null usage fields", () => {
    const event = buildSessionContextUsageUpdatedEvent(context, {
      used_tokens: 42,
      max_tokens: null,
      remaining_tokens: null,
      usage_ratio: null,
      input_tokens: 40,
      output_tokens: 2,
      cache_tokens: 0,
      confidence: "exact",
    }, "example-model");

    expect(event).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      type: "session.context_usage_updated",
      data: {
        context_usage: {
          used_tokens: 42,
          input_tokens: 40,
          output_tokens: 2,
          cache_tokens: 0,
          confidence: "exact",
        },
        model: "example-model",
      },
    });
    expect(event.data.context_usage).not.toHaveProperty("max_tokens");
    expect(event.data.context_usage).not.toHaveProperty("remaining_tokens");
    expect(event.data.context_usage).not.toHaveProperty("usage_ratio");
  });

  test("does not extract context usage from unsupported or empty pi hook payloads", () => {
    expect(contextUsageFromPiEvent({ assistantMessageEvent: { text_delta: "hello" } })).toBeUndefined();
    expect(contextUsageFromPiEvent({ messages: [] })).toBeUndefined();
    expect(contextUsageFromPiEvent({ context_usage: { used_tokens: null, max_tokens: null, confidence: "estimated" } })).toBeUndefined();
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
      context_usage: {
        used_tokens: 1,
        max_tokens: 4,
        remaining_tokens: null,
        usage_ratio: 0.25,
        input_tokens: null,
        output_tokens: null,
        cache_tokens: null,
        confidence: "estimated",
      },
      model: null,
    });
  });

  test("extracts estimated context usage from pi extension context", () => {
    expect(
      contextUsageFromPiContext({
        model: { id: "gpt-5.5" },
        getContextUsage: () => ({ tokens: 6_037, contextWindow: 128_000, percent: 4.716 }),
      }),
    ).toEqual({
      context_usage: {
        used_tokens: 6037,
        max_tokens: 128000,
        remaining_tokens: 121963,
        usage_ratio: 0.04716,
        input_tokens: null,
        output_tokens: null,
        cache_tokens: null,
        confidence: "estimated",
      },
      model: "gpt-5.5",
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
      context_usage: {
        used_tokens: 6037,
        max_tokens: null,
        remaining_tokens: null,
        usage_ratio: null,
        input_tokens: 386,
        output_tokens: 19,
        cache_tokens: 5632,
        confidence: "estimated",
      },
      model: "gpt-5.5",
    });
  });

  test("merges partial pi message usage with context window usage from hook context", () => {
    expect(
      contextUsageFromPiHook(
        {
          message: {
            role: "assistant",
            usage: { input: 386, output: 19, cacheRead: 5632, totalTokens: 6037 },
          },
        },
        {
          model: { id: "gpt-5.5" },
          getContextUsage: () => ({ tokens: 6037, contextWindow: 128000, percent: 4.716 }),
        },
      ),
    ).toEqual({
      context_usage: {
        used_tokens: 6037,
        max_tokens: 128000,
        remaining_tokens: 121963,
        usage_ratio: 0.04716,
        input_tokens: 386,
        output_tokens: 19,
        cache_tokens: 5632,
        confidence: "estimated",
      },
      model: "gpt-5.5",
    });
  });
});

describe("EventReporter", () => {
  test("posts event JSON to the Internal Event API", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ accepted: true, event_id: "evt_server", turn_id: "turn_server" }), { status: 202 }));
    const reporter = new EventReporter({ fetch, logFile: await tempLogFile() });
    const event = buildTurnCompletedEvent(context);

    const result = await reporter.report(context, event);

    expect(result).toEqual({ accepted: true, eventId: "evt_server", turnId: "turn_server" });
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

    const result = await reporter.report(context, buildTurnCompletedEvent(context));

    expect(result).toEqual({ accepted: false });
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

    const result = await reporter.report(context, buildTurnCompletedEvent(context));

    expect(result).toEqual({ accepted: false });
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("internal_event_post_exception");
    expect(log).toContain("network down");
  });
});
