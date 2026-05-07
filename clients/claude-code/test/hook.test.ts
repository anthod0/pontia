import { describe, expect, test, vi } from "vitest";
import { handleClaudeHook } from "../src/hook.js";
import type { LoadTurnContextResult, TurnContext } from "../src/context.js";
import type { InternalEvent } from "../src/events.js";

const context: TurnContext = {
  sessionId: "sess_1",
  turnId: "turn_1",
  clientType: "claude_code",
  internalEventUrl: "http://localhost/internal/v1/events",
};

function dependencies(reportResults: boolean[] = []) {
  const reported: InternalEvent[] = [];
  const report = vi.fn(async (_ctx: TurnContext, event: InternalEvent) => {
    reported.push(event);
    return reportResults.length > 0 ? reportResults.shift()! : true;
  });
  return {
    reported,
    deps: {
      env: {},
      loadContext: vi.fn(async (): Promise<LoadTurnContextResult> => ({ ok: true, context, contextFile: "turn.json", logFile: "hook.log" })),
      makeReporter: vi.fn(() => ({ report })),
      logDiagnostic: vi.fn(async () => undefined),
    },
  };
}

describe("handleClaudeHook", () => {
  test("prompt-submit validates current turn context without reporting events", async () => {
    const { deps, reported } = dependencies();

    const exitCode = await handleClaudeHook("prompt-submit", { hook_event_name: "UserPromptSubmit", prompt: "hi" }, deps);

    expect(exitCode).toBe(0);
    expect(deps.loadContext).toHaveBeenCalled();
    expect(reported).toEqual([]);
  });

  test("stop reports last assistant output then completed", async () => {
    const { deps, reported } = dependencies();

    const exitCode = await handleClaudeHook("stop", { hook_event_name: "Stop", last_assistant_message: "done" }, deps);

    expect(exitCode).toBe(0);
    expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
    expect(reported[0].payload).toEqual({ output: { summary: "done" } });
  });

  test("stop with empty last assistant message still reports completed", async () => {
    const { deps, reported } = dependencies();

    await handleClaudeHook("stop", { hook_event_name: "Stop", last_assistant_message: "" }, deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.completed"]);
  });

  test("stop suppresses completed when output POST fails", async () => {
    const { deps, reported } = dependencies([false]);

    await handleClaudeHook("stop", { hook_event_name: "Stop", last_assistant_message: "done" }, deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.output"]);
  });

  test("stop-failure reports concise Claude Code failure", async () => {
    const { deps, reported } = dependencies();

    await handleClaudeHook("stop-failure", {
      hook_event_name: "StopFailure",
      error: "rate_limit",
      error_details: "429 Too Many Requests",
    }, deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.failed"]);
    expect(reported[0].payload).toEqual({ failure: { message: "Claude Code failed: rate_limit - 429 Too Many Requests" } });
  });

  test("reporting errors are logged and do not produce non-zero exit", async () => {
    const { deps, reported } = dependencies();
    deps.loadContext = vi.fn(async () => ({ ok: false as const, reason: "missing", contextFile: "missing.json", logFile: "hook.log" }));

    const exitCode = await handleClaudeHook("stop", { hook_event_name: "Stop", last_assistant_message: "done" }, deps);

    expect(exitCode).toBe(0);
    expect(reported).toEqual([]);
  });
});
