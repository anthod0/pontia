import { describe, expect, test, vi } from "vitest";
import { createLlmpartyPiExtension } from "../src/index.js";
import { buildLlmpartyTools, internalAgentToolUrl } from "../src/tools.js";
import type { TurnContext } from "../src/context.js";

interface RegisteredTool {
  name: string;
  execute: (toolCallId: string, params: unknown) => Promise<{ content: Array<{ type: "text"; text: string }>; details: unknown }>;
}

const context: TurnContext = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
  internalEventUrl: "http://localhost/internal/v1/events",
};

function install(env: Record<string, string | undefined> = {}, overrides: Partial<Parameters<typeof createLlmpartyPiExtension>[1]> = {}) {
  const tools: RegisteredTool[] = [];
  const pi = {
    on: vi.fn(),
    registerTool: vi.fn((tool: RegisteredTool) => tools.push(tool)),
  };

  createLlmpartyPiExtension(pi as any, {
    env,
    loadContext: vi.fn(async () => ({ ok: true as const, context, contextFile: "turn.json", logFile: "hook.log" })),
    makeReporter: vi.fn(),
    logDiagnostic: vi.fn(async () => undefined),
    ...overrides,
  });

  return { pi, tools };
}

describe("llmparty pi agent tools", () => {
  test("builds four namespaced DAG agent tools from the shared contract", () => {
    const tools = buildLlmpartyTools({
      env: {},
      loadContext: vi.fn(),
      logDiagnostic: vi.fn(),
    });

    expect(tools.map((tool) => tool.name)).toEqual([
      "llmparty_getContext",
      "llmparty_submitPlan",
      "llmparty_submitResult",
      "llmparty_raiseSignal",
    ]);
    expect(tools[0].description).toContain("Fetch the current DAG-managed context");
  });

  test("registers agent-visible llmparty tools from the pi extension", () => {
    const { pi, tools } = install();

    expect(tools.map((tool) => tool.name)).toEqual([
      "llmparty_getContext",
      "llmparty_submitPlan",
      "llmparty_submitResult",
      "llmparty_raiseSignal",
    ]);
    expect(pi.registerTool).toHaveBeenCalledTimes(4);
  });

  test("converts internal event URL to agent tool URL", () => {
    expect(internalAgentToolUrl("http://localhost/internal/v1/events", "submitPlan")).toBe(
      "http://localhost/internal/v1/agent-tools/submitPlan",
    );
    expect(internalAgentToolUrl("http://localhost/internal/v1/events/", "getContext")).toBe(
      "http://localhost/internal/v1/agent-tools/getContext",
    );
  });

  test("tool handler posts current authorized context and input to Internal Agent Tool API", async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(JSON.stringify({ ok: true, tool: "submitPlan", result: { accepted: true, proposal_id: "prop_1" } }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const { tools } = install({}, { fetch: fetchImpl as any });

    const result = await tools.find((tool) => tool.name === "llmparty_submitPlan")!.execute("call_1", {
      mode: "initial_dag",
      summary: "Plan",
      dag: { work_items: [], edges: [] },
    });

    expect(fetchImpl).toHaveBeenCalledWith("http://localhost/internal/v1/agent-tools/submitPlan", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        session_id: "sess_1",
        turn_id: "turn_1",
        runtime_instance_id: "rtinst_1",
        input: { mode: "initial_dag", summary: "Plan", dag: { work_items: [], edges: [] } },
      }),
      signal: undefined,
    });
    expect(result.details).toEqual({ ok: true, tool: "submitPlan", result: { accepted: true, proposal_id: "prop_1" } });
    expect(result.content[0].text).toContain("accepted");
  });

  test("backend error is returned as a tool error without leaking environment", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ error: "not authorized" }), { status: 403, statusText: "Forbidden" }));
    const logDiagnostic = vi.fn(async () => undefined);
    const { tools } = install(
      { LLMPARTY_EXTERNAL_API_TOKEN: "secret-token" },
      { fetch: fetchImpl as any, logDiagnostic },
    );

    const result = await tools.find((tool) => tool.name === "llmparty_getContext")!.execute("call_2", {});

    expect(result.content[0].text).toContain("llmparty_getContext failed: 403 Forbidden");
    expect(result.content[0].text).not.toContain("secret-token");
    expect(logDiagnostic).toHaveBeenCalledWith("hook.log", expect.objectContaining({ code: "agent_tool_post_failed" }));
  });

  test("tool call returns unavailable when current turn context is missing", async () => {
    const fetchImpl = vi.fn();
    const logDiagnostic = vi.fn(async () => undefined);
    const { tools } = install(
      {},
      {
        fetch: fetchImpl as any,
        logDiagnostic,
        loadContext: vi.fn(async () => ({ ok: false as const, reason: "missing context", contextFile: "missing.json", logFile: "hook.log" })),
      },
    );

    const result = await tools.find((tool) => tool.name === "llmparty_submitResult")!.execute("call_3", { status: "completed", summary: "done" });

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(result.content[0].text).toContain("llmparty_submitResult unavailable: missing context");
    expect(logDiagnostic).toHaveBeenCalledWith("hook.log", expect.objectContaining({ code: "agent_tool_context_unavailable" }));
  });
});
