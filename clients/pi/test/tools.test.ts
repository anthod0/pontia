import { describe, expect, test, vi } from "vitest";
import { createLlmpartyPiExtension } from "../src/index.js";

interface RegisteredTool {
  name: string;
  description: string;
  parameters: unknown;
  execute: (toolCallId: string, params: Record<string, unknown>) => Promise<{ content: Array<{ type: string; text: string }>; details?: unknown }>;
}

function install(env: Record<string, string | undefined>, fetchImpl: typeof fetch) {
  const tools: RegisteredTool[] = [];
  const pi = {
    on: vi.fn(),
    registerTool: vi.fn((tool: RegisteredTool) => tools.push(tool)),
  };

  createLlmpartyPiExtension(pi as any, {
    env,
    loadContext: vi.fn(),
    makeReporter: vi.fn(),
    logDiagnostic: vi.fn(),
    fetch: fetchImpl,
  });

  return { pi, tools };
}

function jsonResponse(data: unknown, status = 200): Response {
  return new Response(JSON.stringify({ data, error: null }), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("llmparty pi external API tools", () => {
  test("registers tool definitions from shared llmparty tools manifest", () => {
    const { tools } = install({}, vi.fn() as any);

    expect(tools.map((tool) => tool.name)).toEqual([
      "llmparty_create_session",
      "llmparty_send_message",
      "llmparty_exit_session",
    ]);
    expect(tools[0].description).toContain("Start a new llmparty agent session");
    expect(tools[0].parameters).toMatchObject({ type: "object" });
  });

  test("create session posts to the external API with auth and maps initial_message", async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse({
        session: { session_id: "sess_new" },
        initial_turn: { turn_id: "turn_new" },
      }),
    ) as any;
    const { tools } = install(
      {
        LLMPARTY_EXTERNAL_API_URL: "http://control/external/v1",
        LLMPARTY_EXTERNAL_API_TOKEN: "secret-token",
        LLMPARTY_CLIENT_TYPE: "pi",
        LLMPARTY_WORKSPACE: "/workspace",
      },
      fetchImpl,
    );

    const result = await tools[0].execute("call_1", { initial_message: "start work" });

    expect(fetchImpl).toHaveBeenCalledWith(
      "http://control/external/v1/sessions",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({
          authorization: "Bearer secret-token",
          "content-type": "application/json",
          "idempotency-key": "pi-tool-call_1",
        }),
        body: JSON.stringify({
          client_type: "pi",
          workspace: "/workspace",
          initial_task: { input: "start work", metadata: {} },
          metadata: {},
        }),
      }),
    );
    expect(result.details).toMatchObject({ session_id: "sess_new" });
  });

  test("send message posts inbox message and maps message to input", async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse({ inbox_message: { message_id: "msg_1", state: "pending" } }),
    ) as any;
    const { tools } = install(
      {
        LLMPARTY_EXTERNAL_API_URL: "http://control/external/v1/",
        LLMPARTY_EXTERNAL_API_TOKEN: "secret-token",
      },
      fetchImpl,
    );

    const result = await tools[1].execute("call_2", {
      session_id: "sess_1",
      message: "please continue",
      delivery_policy: "interrupt_now",
    });

    expect(fetchImpl).toHaveBeenCalledWith(
      "http://control/external/v1/sessions/sess_1/inbox/messages",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ input: "please continue", delivery_policy: "interrupt_now", metadata: {} }),
      }),
    );
    expect(result.details).toMatchObject({ message_id: "msg_1" });
  });

  test("exit session terminates only the current runtime session", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse({ session: { session_id: "sess_self", state: "terminated" } })) as any;
    const { tools } = install(
      {
        LLMPARTY_EXTERNAL_API_URL: "http://control/external/v1",
        LLMPARTY_EXTERNAL_API_TOKEN: "secret-token",
        LLMPARTY_SESSION_ID: "sess_self",
      },
      fetchImpl,
    );

    const result = await tools[2].execute("call_3", { reason: "done" });

    expect(fetchImpl).toHaveBeenCalledWith(
      "http://control/external/v1/sessions/sess_self/terminate",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ reason: "done" }) }),
    );
    expect(result.details).toMatchObject({ session_id: "sess_self" });
  });

  test("tools return explicit error when external API environment is missing", async () => {
    const fetchImpl = vi.fn() as any;
    const { tools } = install({}, fetchImpl);

    const result = await tools[1].execute("call_4", { session_id: "sess_1", message: "hello" });

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(result.content[0].text).toContain("LLMPARTY_EXTERNAL_API_URL is required");
  });
});
