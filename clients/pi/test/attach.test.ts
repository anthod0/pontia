import { describe, expect, test, vi } from "vitest";
import { attachRuntimeBinding } from "../src/attach.js";

describe("attachRuntimeBinding", () => {
  test("upserts pi session with tmux metadata and returns bound context", async () => {
    const fetchImpl = vi.fn(async (_url: string, init?: RequestInit) => {
      expect(init?.method).toBe("POST");
      const body = JSON.parse(String(init?.body));
      expect(body).toMatchObject({
        client_type: "pi",
        client_session_key: "pi_session_1",
        client_session_file: "/tmp/pi/session.jsonl",
        client_session_dir: "/tmp/pi",
        client_cwd: "/workspace",
        launch_cwd: "/workspace",
        runtime_instance_id: expect.stringMatching(/^rtinst_/),
        tmux: {
          socket_path: "/tmp/tmux-1000/default",
          session_id: "$1",
          session_name: "dev",
          window_id: "@3",
          window_index: 0,
          pane_id: "%42",
          pane_index: 1,
          pane_current_path: "/workspace",
        },
      });
      return new Response(JSON.stringify({
        session: { session_id: "sess_bound" },
        runtime: {
          runtime_instance_id: body.runtime_instance_id,
          internal_event_url: "http://127.0.0.1:8080/internal/v1/events",
          current_turn_file: "/tmp/pontia/current-turn.json",
          capabilities: { accept_task: true },
        },
      }), { status: 200 });
    });

    const result = await attachRuntimeBinding({
      env: {
        PONTIA_INTERNAL_API_URL: "http://127.0.0.1:8080/internal/v1",
        TMUX: "/tmp/tmux-1000/default,123,0",
        TMUX_PANE: "%42",
      },
      session: {
        clientSessionKey: "pi_session_1",
        clientSessionFile: "/tmp/pi/session.jsonl",
        clientSessionDir: "/tmp/pi",
        clientCwd: "/workspace",
      },
      fetch: fetchImpl as any,
      runTmuxDisplayMessage: vi.fn(async () => "$1 dev @3 0 %42 1 /workspace"),
    });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected bound context");
    expect(result.context).toMatchObject({
      sessionId: "sess_bound",
      clientType: "pi",
      internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
      currentTurnFile: "/tmp/pontia/current-turn.json",
    });
    expect(fetchImpl).toHaveBeenCalledWith(
      "http://127.0.0.1:8080/internal/v1/runtime-bindings/upsert",
      expect.objectContaining({ method: "POST" }),
    );
  });

  test("returns unbound when pontia server is unavailable", async () => {
    const result = await attachRuntimeBinding({
      env: { PONTIA_INTERNAL_API_URL: "http://127.0.0.1:9/internal/v1" },
      session: { clientSessionKey: "pi_session_1", clientCwd: "/workspace" },
      fetch: vi.fn(async () => { throw new Error("connect ECONNREFUSED"); }) as any,
    });

    expect(result).toMatchObject({ ok: false, code: "upsert_failed" });
  });
});
