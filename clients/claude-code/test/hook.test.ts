import { describe, expect, test, vi } from "vitest";
import { handleClaudeHook } from "../src/hook.js";
import type { TurnContext } from "../src/context.js";
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
      loadContext: vi.fn(async (): Promise<unknown> => ({ ok: true, context })),
      makeReporter: vi.fn(() => ({ report })),
      logDiagnostic: vi.fn(async () => undefined),
    },
  };
}

describe("handleClaudeHook", () => {
  test("session-start reports one-time agent client ready from runtime env", async () => {
    const { deps, reported } = dependencies();
    deps.env = {
      PONTIA_SESSION_ID: "sess_ready",
      PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      PONTIA_CLAUDE_HOOK_LOG: "/tmp/claude-hook.log",
    };

    const exitCode = await handleClaudeHook("session-start", { hook_event_name: "SessionStart", source: "startup" }, deps);

    expect(exitCode).toBe(0);
    expect(deps.loadContext).not.toHaveBeenCalled();
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_ready",
      turn_id: null,
      source: "agent_client",
      client_type: "claude_code",
      payload: { runtime_instance_id: "rtinst_1" },
    });
  });

  test("prompt-submit is ignored without loading current turn context", async () => {
    const { deps, reported } = dependencies();

    const exitCode = await handleClaudeHook("prompt-submit", { hook_event_name: "UserPromptSubmit", prompt: "hi" }, deps);

    expect(exitCode).toBe(0);
    expect(deps.loadContext).not.toHaveBeenCalled();
    expect(reported).toEqual([]);
  });

  test("stop is ignored without loading current turn context", async () => {
    const { deps, reported } = dependencies();

    const exitCode = await handleClaudeHook("stop", { hook_event_name: "Stop", last_assistant_message: "done" }, deps);

    expect(exitCode).toBe(0);
    expect(deps.loadContext).not.toHaveBeenCalled();
    expect(reported).toEqual([]);
  });

  test("stop-failure is ignored without loading current turn context", async () => {
    const { deps, reported } = dependencies();

    const exitCode = await handleClaudeHook("stop-failure", {
      hook_event_name: "StopFailure",
      error: "rate_limit",
      error_details: "429 Too Many Requests",
    }, deps);

    expect(exitCode).toBe(0);
    expect(deps.loadContext).not.toHaveBeenCalled();
    expect(reported).toEqual([]);
  });
});
