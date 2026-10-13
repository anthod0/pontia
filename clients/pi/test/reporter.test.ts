import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import {
  buildSessionContextUsageUpdatedEvent,
  buildTurnCompletedEvent,
  buildTurnFailedEvent,
  buildTurnOutputEvent,
  buildTurnStartedEvent,
  contextUsageFromPiContext,
  contextUsageFromPiEvent,
  contextUsageFromPiHook,
} from "../src/events.js";
import { RpcError } from "../src/control-socket.js";
import { EventReporter } from "../src/reporter.js";
import { tempDir } from "./temp-dir.js";

async function tempLogFile() {
  return join(await tempDir("pontia-pi-reporter-"), "pi-hook.log");
}

const context = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeId: "rtinst_1",
  clientType: "pi",
} as const;

describe("event builders", () => {
  test("builds a turn.started observation without Pontia-owned identity", () => {
    const event = buildTurnStartedEvent(
      { ...context, input: "from web", inboxMessageId: "msg_1" },
      "entry_1",
    );

    expect(event).toEqual({
      session_id: "sess_1",
      report_key: "entry_1",
      type: "turn.started",
      data: {
        runtime_id: "rtinst_1",
        input_summary: "from web",
        previous_leaf_id: null,
        inbox_message_id: "msg_1",
      },
    });
    for (const field of ["event_id", "turn_id", "source", "client_type", "time"]) {
      expect(event).not.toHaveProperty(field);
    }
  });

  test("bounds the input summary without changing the actual Unicode prompt", () => {
    const input = "中😀".repeat(30_000);
    const event = buildTurnStartedEvent({ ...context, input }, "entry_1");
    expect(event.data.input_summary).toBe("中😀".repeat(100));
    expect(input.length).toBe(90_000);
    expect(buildTurnStartedEvent(context, "entry_1").data.input_summary).toBeUndefined();
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
      data: { terminal_leaf_id: null },
    });
    expect(buildTurnCompletedEvent(context).data).not.toHaveProperty("runtime_id");
    expect(buildTurnFailedEvent(context, "boom")).toMatchObject({
      type: "turn.failed",
      data: { failure_message: "boom" },
    });
    expect(buildTurnFailedEvent(context, "boom").data).not.toHaveProperty("runtime_id");
  });

  test("truncates turn.output summaries to 200 Unicode characters", () => {
    const event = buildTurnOutputEvent(context, "界".repeat(201));
    expect(event.data).toEqual({ output_summary: "界".repeat(200) });
  });

  test("builds session.context_usage_updated observation without null usage fields", () => {
    const event = buildSessionContextUsageUpdatedEvent(
      context,
      {
        used_tokens: 42,
        max_tokens: null,
        remaining_tokens: null,
        usage_ratio: null,
        input_tokens: 40,
        output_tokens: 2,
        cache_tokens: 0,
        confidence: "exact",
      },
      "example-model",
    );

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
    expect(
      contextUsageFromPiEvent({ assistantMessageEvent: { text_delta: "hello" } }),
    ).toBeUndefined();
    expect(contextUsageFromPiEvent({ messages: [] })).toBeUndefined();
    expect(
      contextUsageFromPiEvent({
        context_usage: { used_tokens: null, max_tokens: null, confidence: "estimated" },
      }),
    ).toBeUndefined();
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
  test("reports facts over RPC and returns the canonical turn identity", async () => {
    const request = vi.fn(async () => ({
      accepted: true,
      event_id: "evt_server",
      turn_id: "turn_server",
    }));
    const reporter = new EventReporter({ connection: { request }, logFile: await tempLogFile() });
    const event = buildTurnCompletedEvent(context);
    expect(await reporter.report(context, event)).toEqual({
      accepted: true,
      eventId: "evt_server",
      turnId: "turn_server",
    });
    expect(request).toHaveBeenCalledWith("event.report", {
      runtime_id: "rtinst_1",
      event,
    });
  });

  test("retries turn.started with the same report key after a lost response", async () => {
    const event = buildTurnStartedEvent(context, "entry_1");
    const request = vi
      .fn()
      .mockRejectedValueOnce(new Error("response lost"))
      .mockResolvedValueOnce({
        accepted: true,
        duplicate: true,
        event_id: "evt_server",
        turn_id: "turn_server",
      });
    const reporter = new EventReporter({ connection: { request }, logFile: await tempLogFile() });

    expect(await reporter.report(context, event)).toEqual({
      accepted: true,
      eventId: "evt_server",
      turnId: "turn_server",
    });
    expect(request).toHaveBeenCalledTimes(2);
    expect(request.mock.calls[0]).toEqual(request.mock.calls[1]);
  });

  test.each([new RpcError(-32603, "server error"), new Error("connection lost")])(
    "retries uncertain turn.started failures at most three times",
    async (error) => {
      const logFile = await tempLogFile();
      const request = vi.fn(async () => {
        throw error;
      });
      const reporter = new EventReporter({ connection: { request }, logFile });
      expect(await reporter.report(context, buildTurnStartedEvent(context, "entry_1"))).toEqual({
        accepted: false,
      });
      expect(request).toHaveBeenCalledTimes(3);
      expect(await readFile(logFile, "utf8")).toContain("pi_event_report_failed");
    },
  );

  test("does not retry an explicit turn.started rejection", async () => {
    const request = vi.fn(async () => {
      throw new RpcError(-32009, "stale");
    });
    const reporter = new EventReporter({ connection: { request }, logFile: await tempLogFile() });
    expect(await reporter.report(context, buildTurnStartedEvent(context, "entry_1"))).toEqual({
      accepted: false,
    });
    expect(request).toHaveBeenCalledTimes(1);
  });

  test.each([new RpcError(-32603, "server error"), new Error("connection lost")])(
    "logs report failures without inventing acceptance",
    async (error) => {
      const logFile = await tempLogFile();
      const request = vi.fn(async () => {
        throw error;
      });
      const reporter = new EventReporter({ connection: { request }, logFile });
      expect(await reporter.report(context, buildTurnCompletedEvent(context))).toEqual({
        accepted: false,
      });
      expect(request).toHaveBeenCalledTimes(1);
      const log = await readFile(logFile, "utf8");
      expect(log).toContain("pi_event_report_failed");
      expect(log).toContain(error.message);
    },
  );
});
