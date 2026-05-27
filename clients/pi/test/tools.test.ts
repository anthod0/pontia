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
  test("builds DAG agent tools from the shared contract without a pi-only prefix", () => {
    const tools = buildLlmpartyTools({
      env: {},
      loadContext: vi.fn(),
      logDiagnostic: vi.fn(),
    });

    expect(tools.map((tool) => tool.name)).toEqual([
      "getContext",
      "submitPlan",
      "applyPlan",
      "submitResult",
      "raiseSignal",
    ]);
    expect(tools[0].description).toContain("Fetch the current DAG-managed context");
  });

  test("does not register llmparty tools when agent kind is missing", () => {
    const { pi, tools } = install();

    expect(tools.map((tool) => tool.name)).toEqual([]);
    expect(pi.registerTool).not.toHaveBeenCalled();
  });

  test("does not register llmparty tools when agent kind is unsupported", () => {
    const { pi, tools } = install({ LLMPARTY_AGENT_KIND: "unknown" });

    expect(tools.map((tool) => tool.name)).toEqual([]);
    expect(pi.registerTool).not.toHaveBeenCalled();
  });

  test("planner agent kind registers planning tools only", () => {
    const { pi, tools } = install({ LLMPARTY_AGENT_KIND: "planner" });

    expect(tools.map((tool) => tool.name)).toEqual(["getContext", "submitPlan", "applyPlan", "raiseSignal"]);
    expect(pi.registerTool).toHaveBeenCalledTimes(4);
  });

  test("executor agent kind registers execution tools only", () => {
    const { pi, tools } = install({ LLMPARTY_AGENT_KIND: "executor" });

    expect(tools.map((tool) => tool.name)).toEqual(["getContext", "submitResult", "raiseSignal"]);
    expect(pi.registerTool).toHaveBeenCalledTimes(3);
  });

  test("converts internal event URL to agent tool URL", () => {
    expect(internalAgentToolUrl("http://localhost/internal/v1/events", "submitPlan")).toBe(
      "http://localhost/internal/v1/agent-tools/submitPlan",
    );
    expect(internalAgentToolUrl("http://localhost/internal/v1/events/", "getContext")).toBe(
      "http://localhost/internal/v1/agent-tools/getContext",
    );
  });

  test("submitPlan schema matches backend WorkItem draft contract", () => {
    const tools = buildLlmpartyTools({
      env: {},
      loadContext: vi.fn(),
      logDiagnostic: vi.fn(),
    });
    const submitPlan = tools.find((tool) => tool.name === "submitPlan")! as any;
    const workItem = submitPlan.parameters.properties.dag.properties.work_items.items;

    expect(workItem.required).toEqual([
      "temp_id",
      "title",
      "description",
      "kind",
      "action",
      "execution_profile_id",
    ]);
    expect(workItem.properties.kind.enum).toEqual([
      "design",
      "implementation",
      "review",
      "test",
      "debug",
      "documentation",
      "planning",
      "other",
    ]);
    expect(workItem.properties.action.enum).toEqual(["agent_turn", "human_input", "noop"]);
    expect(submitPlan.parameters.properties.risks.items.properties.summary.type).toBe("string");
  });

  test("tool handler posts current authorized context and input to Internal Agent Tool API", async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(JSON.stringify({ ok: true, tool: "submitPlan", result: { accepted: true, proposal_id: "prop_1" } }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const { tools } = install({ LLMPARTY_AGENT_KIND: "planner" }, { fetch: fetchImpl as any });

    const result = await tools.find((tool) => tool.name === "submitPlan")!.execute("call_1", {
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
    expect(result.content[0].text).toBe("submitPlan succeeded.\nAccepted: yes\nProposal ID: prop_1");
    expect(result.content[0].text).not.toContain("{");
  });

  test("all tool handlers return agent-visible plain text instead of JSON", async () => {
    const responses: Record<string, unknown> = {
      getContext: { ok: true, tool: "getContext", result: { text: "llmparty context: execution\n\nCurrent WorkItem:\n- Title: Do it" } },
      submitPlan: { ok: true, tool: "submitPlan", result: { accepted: true, proposal_id: "prop_1" } },
      applyPlan: { ok: true, tool: "applyPlan", result: { proposal_id: "prop_1" } },
      submitResult: { ok: true, tool: "submitResult", result: { recorded: true, run_id: "run_1", status: "completed" } },
      raiseSignal: { ok: true, tool: "raiseSignal", result: { signal_id: "sig_1", recorded: true } },
    };
    const fetchImpl = vi.fn(async (url: string) => {
      const toolName = url.split("/").at(-1)!;
      return new Response(JSON.stringify(responses[toolName]), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });
    const { tools } = install({ LLMPARTY_AGENT_KIND: "planner" }, { fetch: fetchImpl as any });

    for (const tool of tools) {
      const result = await tool.execute("call_plain", { status: "completed", summary: "done" });

      expect(result.content[0].type).toBe("text");
      expect(result.content[0].text).not.toContain("{");
      expect(result.content[0].text).not.toContain("\"");
    }
  });

  test("getContext returns agent-visible plain text instead of JSON", async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(JSON.stringify({ ok: true, tool: "getContext", result: { text: "llmparty context: execution\n\nCurrent WorkItem:\n- Title: Do it" } }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const { tools } = install({ LLMPARTY_AGENT_KIND: "executor" }, { fetch: fetchImpl as any });

    const result = await tools.find((tool) => tool.name === "getContext")!.execute("call_context", {});

    expect(result.content[0].text).toBe("llmparty context: execution\n\nCurrent WorkItem:\n- Title: Do it");
    expect(result.content[0].text).not.toContain("{\n");
    expect(result.details).toEqual({ ok: true, tool: "getContext", result: { text: "llmparty context: execution\n\nCurrent WorkItem:\n- Title: Do it" } });
  });

  test("backend error is returned as a tool error without leaking environment", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ error: "not authorized" }), { status: 403, statusText: "Forbidden" }));
    const logDiagnostic = vi.fn(async () => undefined);
    const { tools } = install(
      { LLMPARTY_AGENT_KIND: "executor", LLMPARTY_EXTERNAL_API_TOKEN: "secret-token" },
      { fetch: fetchImpl as any, logDiagnostic },
    );

    const result = await tools.find((tool) => tool.name === "getContext")!.execute("call_2", {});

    expect(result.content[0].text).toContain("getContext failed: 403 Forbidden");
    expect(result.content[0].text).toContain("not authorized");
    expect(result.content[0].text).not.toContain("{");
    expect(result.content[0].text).not.toContain("secret-token");
    expect(logDiagnostic).toHaveBeenCalledWith("hook.log", expect.objectContaining({ code: "agent_tool_post_failed" }));
  });

  test("tool call returns unavailable when current turn context is missing", async () => {
    const fetchImpl = vi.fn();
    const logDiagnostic = vi.fn(async () => undefined);
    const { tools } = install(
      { LLMPARTY_AGENT_KIND: "executor" },
      {
        fetch: fetchImpl as any,
        logDiagnostic,
        loadContext: vi.fn(async () => ({ ok: false as const, reason: "missing context", contextFile: "missing.json", logFile: "hook.log" })),
      },
    );

    const result = await tools.find((tool) => tool.name === "submitResult")!.execute("call_3", { status: "completed", summary: "done" });

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(result.content[0].text).toContain("submitResult unavailable: missing context");
    expect(logDiagnostic).toHaveBeenCalledWith("hook.log", expect.objectContaining({ code: "agent_tool_context_unavailable" }));
  });
});
