import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import {
  buildSessionContextUsageUpdatedEvent,
  buildSessionReadyEvent,
  buildSessionExitedEvent,
  buildTurnInterruptedEvent,
  buildTurnCompletedEvent,
  buildTurnFailedEvent,
  buildTurnOutputEvent,
  buildTurnStartedEvent,
  contextUsageFromPiContext,
  contextUsageFromPiEvent,
  contextUsageFromPiHook,
} from "../src/events.js";
import { AttachmentRejected, RpcError } from "../shared/control-socket.js";
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
      report_key: expect.stringMatching(/\S/),
      data: { runtime_id: context.runtimeId, output_summary: "hello" },
    });
    expect(buildTurnCompletedEvent(context)).toMatchObject({
      turn_id: "turn_1",
      type: "turn.completed",
      data: { terminal_leaf_id: null },
    });
    expect(buildTurnCompletedEvent(context).data.runtime_id).toBe(context.runtimeId);
    expect(buildTurnFailedEvent(context, "boom")).toMatchObject({
      type: "turn.failed",
      data: { failure_message: "boom" },
    });
    expect(buildTurnFailedEvent(context, "boom").data.runtime_id).toBe(context.runtimeId);
  });

  test("truncates turn.output summaries to 200 Unicode characters", () => {
    const event = buildTurnOutputEvent(context, "界".repeat(201));
    expect(event.data).toEqual({ runtime_id: context.runtimeId, output_summary: "界".repeat(200) });
  });

  test("assigns distinct keys and captures the runtime when each observation is made", () => {
    const events = [
      buildSessionReadyEvent(context),
      buildSessionExitedEvent(context, "quit"),
      buildTurnOutputEvent(context, "a"),
      buildTurnOutputEvent(context, "a"),
      buildTurnCompletedEvent(context),
      buildTurnInterruptedEvent(context),
      buildTurnFailedEvent(context, "failure"),
    ];
    for (const event of events) {
      expect(event.report_key).toMatch(/\S/);
      expect(event.data.runtime_id).toBe(context.runtimeId);
    }
    expect(new Set(events.map((event) => event.report_key)).size).toBe(events.length);
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
      report_key: expect.stringMatching(/\S/),
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

  test("drains offline facts in order after repeated outages, a lost ACK and a conflict", async () => {
    const logFile = await tempLogFile();
    const output = buildTurnOutputEvent(context, "answer");
    const terminal = buildTurnCompletedEvent(context);
    const calls: string[] = [];
    const stored = new Set<string>();
    const request = vi.fn(async (_method, { event }) => {
      calls.push(event.report_key);
      if (calls.length <= 3) throw new Error("offline");
      if (event === output) throw new RpcError(-32009, "conflict");
      const duplicate = stored.has(event.report_key);
      stored.add(event.report_key);
      if (!duplicate) throw new Error("ack lost");
      return { accepted: true, duplicate };
    });
    const reporter = new EventReporter({ connection: { request }, logFile });
    const [failed, done] = await Promise.all([
      reporter.report(context, output),
      reporter.report(context, terminal),
    ]);
    expect(failed.conflict).toBe(true);
    expect(done.accepted).toBe(true);
    expect([...stored]).toEqual([terminal.report_key]);
    expect(calls).toEqual([
      output.report_key,
      output.report_key,
      output.report_key,
      output.report_key,
      terminal.report_key,
      terminal.report_key,
    ]);
  });

  test("does not retry an explicit turn.started rejection", async () => {
    const request = vi.fn(async () => {
      throw new RpcError(-32009, "stale");
    });
    const reporter = new EventReporter({ connection: { request }, logFile: await tempLogFile() });
    expect(await reporter.report(context, buildTurnStartedEvent(context, "entry_1"))).toEqual({
      accepted: false,
      conflict: true,
    });
    expect(request).toHaveBeenCalledTimes(1);
  });

  test("a permanent attachment rejection discards all remaining facts", async () => {
    const logFile = await tempLogFile();
    const request = vi.fn().mockRejectedValue(new AttachmentRejected("Session exited"));
    const reporter = new EventReporter({ connection: { request }, logFile });
    const events = [buildTurnOutputEvent(context, "answer"), buildTurnCompletedEvent(context)];
    const results = await Promise.all(events.map((event) => reporter.report(context, event)));
    expect(results.map((result) => result.accepted)).toEqual([false, false]);
    expect(request).toHaveBeenCalledTimes(1);
    if (process.platform !== "win32") expect((await stat(logFile)).mode & 0o777).toBe(0o600);
    const log = await readFile(logFile, "utf8");
    for (const event of events) expect(log).toContain(event.report_key);
  });
});
