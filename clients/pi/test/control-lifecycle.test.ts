import { access, readdir, realpath, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import type { InternalEvent } from "../src/events.js";
import { tempDir } from "./temp-dir.js";

test("extension publishes only listening endpoints, replaces them on session switches, and isolates registration failure", async () => {
  const root = await tempDir("pc-");
  const workspace = await realpath(root);
  await writeFile(join(root, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');
  const handlers: Record<string, (event: any, context?: any) => Promise<void>> = {};
  const events: InternalEvent[] = [];
  const published: Array<{ session_id: string; runtime_instance_id: string; socket_path: string }> = [];
  const logDiagnostic = vi.fn(async () => {});
  const fetchImpl = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith("/workspaces")) return Response.json({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } });
    if (url.includes("/session-context?")) return new Response("", { status: 404 });
    const body = JSON.parse(String(init?.body));
    if (url.endsWith("/upsert")) return Response.json({
      session: { session_id: `sess_${body.client_session_key}` },
      runtime: { runtime_instance_id: `rtinst_${body.client_session_key}`, internal_event_url: "http://localhost/internal/v1/events" },
    });
    if (url.endsWith("/pi-control")) {
      await access(body.socket_path);
      published.push(body);
      return new Response("", { status: body.session_id === "sess_failed" ? 409 : 200 });
    }
    throw new Error(`Unexpected URL ${url}`);
  });
  createPontiaPiExtension({
    on(name: string, handler: any) { handlers[name] = handler; },
    registerCommand() {},
  } as any, {
    env: { PONTIA_HOME: root, XDG_RUNTIME_DIR: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
    fetch: fetchImpl as typeof fetch,
    isManagedPane: async () => true,
    makeReporter: () => ({ report: async (_context, event) => {
      if (event.type === "session.ready") {
        expect(published.at(-1)?.session_id).toBe(event.session_id);
      }
      events.push(event);
      return true;
    } }),
    logDiagnostic,
  });
  const context = (id: string) => ({ mode: "tui", sessionManager: {
    getSessionId: () => id, getSessionFile: () => join(root, `${id}.jsonl`), getCwd: () => workspace,
  } });
  try {
    await handlers.session_start({ reason: "startup" }, context("one"));
    expect(published[0]).toMatchObject({ session_id: "sess_one", runtime_instance_id: "rtinst_one" });
    await handlers.session_shutdown({ reason: "new" });
    await expect(access(published[0].socket_path)).rejects.toThrow();
    await handlers.session_start({ reason: "new" }, context("two"));
    expect(published[1]).toMatchObject({ session_id: "sess_two", runtime_instance_id: "rtinst_two" });
    expect(published[1].socket_path).not.toBe(published[0].socket_path);
    await handlers.session_shutdown({ reason: "new" });
    await handlers.session_start({ reason: "new" }, context("failed"));
    expect(published).toHaveLength(3);
    await expect(access(published[2].socket_path)).rejects.toThrow();
    expect(events.map((event) => event.type)).toEqual(["session.ready", "session.exited", "session.ready", "session.exited", "session.ready"]);
    expect(logDiagnostic).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ code: "pi_control_unavailable" }));
    expect(await readdir(root)).toEqual(["config.toml"]);
  } finally {
    await handlers.session_shutdown({ reason: "quit" });
  }
});
