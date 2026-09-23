import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import { loadTurnContext } from "../src/context.js";
import { tempDir } from "./temp-dir.js";

const sessionContext = { sessionId: "sess_rpc", runtimeInstanceId: "rt_rpc", clientType: "pi" as const };

describe("loadTurnContext", () => {
  test("claims pending input over RPC without HTTP configuration or a turn id", async () => {
    const root = await tempDir("pi-context-");
    const request = vi.fn(async () => ({ current_turn: {
      session_id: "sess_rpc", runtime_instance_id: "rt_rpc", client_type: "pi",
      input: "from web ui", inbox_message_id: "msg_rpc",
    } }));
    const result = await loadTurnContext({ PONTIA_HOME: root }, { connection: { request }, sessionContext });
    expect(request).toHaveBeenCalledExactlyOnceWith("turn.claim", {
      session_id: "sess_rpc", runtime_instance_id: "rt_rpc", client_type: "pi",
    });
    expect(result).toEqual({ ok: true, logFile: join(root, "state/pi-hook.log"), context: {
      ...sessionContext, turnId: undefined, input: "from web ui", inboxMessageId: "msg_rpc",
    } });
  });

  test.each(["empty", "disconnected", "unbound"])("silently skips a %s claim", async (kind) => {
    const root = await tempDir("pi-context-");
    const request = vi.fn(async () => {
      if (kind === "disconnected") throw new Error("connection closed");
      return { current_turn: null };
    });
    const result = await loadTurnContext({ PONTIA_HOME: root }, {
      connection: { request }, sessionContext: kind === "unbound" ? undefined : sessionContext,
    });
    expect(result).toMatchObject({ ok: false, silent: true, logFile: join(root, "state/pi-hook.log") });
    expect(request).toHaveBeenCalledTimes(kind === "unbound" ? 0 : 1);
  });

  test("rejects invalid claim identities", async () => {
    const root = await tempDir("pi-context-");
    const result = await loadTurnContext({ PONTIA_HOME: root }, {
      connection: { request: async () => ({ current_turn: { client_type: "generic" } }) }, sessionContext,
    });
    expect(result).toMatchObject({ ok: false, reason: "session_id is required; client_type must be pi; runtime_instance_id is required" });
  });
});
