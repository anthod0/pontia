import { mkdtempSync, writeFileSync } from "node:fs";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, test, vi } from "vitest";
import { runClaudeHook, type ClaudeHookInput } from "../src/hook.js";
import type { InternalEvent } from "../src/events.js";

const tmpDirs: string[] = [];
const defaultPontiaHome = mkdtempSync(join(tmpdir(), "pontia-claude-hook-home-"));
writeFileSync(join(defaultPontiaHome, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');

async function tempDir() {
  const dir = await mkdtemp(join(tmpdir(), "pontia-claude-hook-"));
  tmpDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
  vi.restoreAllMocks();
});

function baseInput(overrides: Partial<ClaudeHookInput> = {}): ClaudeHookInput {
  return {
    hook_event_name: "SessionStart",
    session_id: "claude_session_1",
    transcript_path: "/tmp/claude/session.jsonl",
    cwd: "/repo",
    ...overrides,
  };
}

function install(overrides: Partial<Parameters<typeof runClaudeHook>[1]> = {}) {
  const reported: InternalEvent[] = [];
  const diagnostics: unknown[] = [];
  return {
    reported,
    diagnostics,
    deps: {
      env: { PONTIA_HOME: defaultPontiaHome, ...(overrides.env ?? {}) },
      makeReporter: vi.fn(() => ({ report: vi.fn(async (_ctx: unknown, event: InternalEvent) => {
        reported.push(event);
        return {
          accepted: true,
          turnId: event.type === "turn.started" ? "turn_canonical" : event.turn_id,
        };
      }) })),
      logDiagnostic: vi.fn(async (_logFile: string, entry: unknown) => {
        diagnostics.push(entry);
      }),
      ...overrides,
    },
  };
}

describe("pontia claude hook", () => {
  test("SessionStart reports ready from managed runtime env without external workspace discovery", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn();
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_ready", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
      fetch: fetchImpl as any,
    });

    await runClaudeHook(baseInput({ cwd: workspace }), deps);

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toEqual({
      session_id: "sess_ready",
      type: "session.ready",
      data: {
        runtime_instance_id: "rtinst_1",
        client_session_key: "claude_session_1",
        transcript_path: "/tmp/claude/session.jsonl",
        client_cwd: workspace,
      },
    });
  });

  test("manual SessionStart creates a binding in an active workspace and reports ready", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      if (url.startsWith("http://localhost/internal/v1/agent-bindings/session-context?")) {
        return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
      }
      expect(url).toBe("http://localhost/internal/v1/runtime-bindings/upsert");
      expect(init?.method).toBe("POST");
      const request = JSON.parse(String(init?.body));
      expect(request).toMatchObject({ client_type: "claude", client_session_key: "claude_session_1", client_cwd: workspace });
      expect(request.runtime_instance_id).toMatch(/^rtinst_/);
      return new Response(JSON.stringify({
        session: { session_id: "sess_manual" },
        runtime: { runtime_instance_id: request.runtime_instance_id, internal_event_url: "http://localhost/internal/v1/events" },
      }), { status: 200 });
    });
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ cwd: workspace }), deps);

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_manual",
      data: { client_session_key: "claude_session_1", runtime_instance_id: expect.stringMatching(/^rtinst_/) },
    });
  });

  test("manual SessionStart no-ops when workspace is not active", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 }));
    const { deps, reported, diagnostics } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ cwd: workspace }), deps);

    expect(reported).toEqual([]);
    expect(diagnostics).toEqual([expect.objectContaining({ code: "workspace_not_active" })]);
  });

  test("UserPromptSubmit claims managed pending turn and reports turn.started", async () => {
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      expect(url).toBe("http://localhost/internal/v1/sessions/sess_1/current-turn/claim");
      expect(JSON.parse(String(init?.body))).toMatchObject({ runtime_instance_id: "rtinst_1", client_type: "claude" });
      return new Response(JSON.stringify({ data: { current_turn: {
        session_id: "sess_1",
        turn_id: "turn_1",
        client_type: "claude",
        runtime_instance_id: "rtinst_1",
        internal_event_url: "http://localhost/internal/v1/events",
      } } }), { status: 200 });
    });
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_1", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
      fetch: fetchImpl as any,
    });

    await runClaudeHook(baseInput({ hook_event_name: "UserPromptSubmit", prompt: "build it" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
    expect(reported[0]).toEqual({
      session_id: "sess_1",
      type: "turn.started",
      data: { runtime_instance_id: "rtinst_1", input: { summary: "build it" } },
    });
  });

  test("UserPromptSubmit lets pontia assign the canonical turn_id when the pending context omits it", async () => {
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      expect(url).toBe("http://localhost/internal/v1/sessions/sess_1/current-turn/claim");
      expect(JSON.parse(String(init?.body))).toMatchObject({ runtime_instance_id: "rtinst_1", client_type: "claude" });
      return new Response(JSON.stringify({ data: { current_turn: {
        session_id: "sess_1",
        client_type: "claude",
        runtime_instance_id: "rtinst_1",
        internal_event_url: "http://localhost/internal/v1/events",
      } } }), { status: 200 });
    });
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_1", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
      fetch: fetchImpl as any,
    });

    await runClaudeHook(baseInput({ hook_event_name: "UserPromptSubmit", prompt: "build it" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
    expect(reported[0]).toEqual({
      session_id: "sess_1",
      type: "turn.started",
      data: { runtime_instance_id: "rtinst_1", input: { summary: "build it" } },
    });
  });

  test("UserPromptSubmit reports managed TUI prompt when no pending backend turn exists", async () => {
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      expect(url).toBe("http://localhost/internal/v1/sessions/sess_1/current-turn/claim");
      expect(JSON.parse(String(init?.body))).toMatchObject({ runtime_instance_id: "rtinst_1", client_type: "claude" });
      return new Response(JSON.stringify({ data: { current_turn: null } }), { status: 200 });
    });
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_1", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
      fetch: fetchImpl as any,
    });

    await runClaudeHook(baseInput({ hook_event_name: "UserPromptSubmit", prompt: "typed in tui" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
    expect(reported[0]).toEqual({
      session_id: "sess_1",
      type: "turn.started",
      data: { runtime_instance_id: "rtinst_1", input: { summary: "typed in tui" } },
    });
  });

  test("Stop resolves current turn by Claude session id, reports final output then completed", async () => {
    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://localhost/internal/v1/agent-bindings/current-turn?client_type=claude&client_session_key=claude_session_1");
      return new Response(JSON.stringify({ data: { current_turn: {
        session_id: "sess_1",
        turn_id: "turn_1",
        client_type: "claude",
        client_session_key: "claude_session_1",
        runtime_instance_id: "rtinst_1",
        internal_event_url: "http://localhost/internal/v1/events",
      } } }), { status: 200 });
    });
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ hook_event_name: "Stop", last_assistant_message: "done" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      data: { output: { summary: "done" } },
    });
  });

  test("Stop does not infer output from the transcript when last_assistant_message is missing", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { current_turn: {
      session_id: "sess_1",
      turn_id: "turn_1",
      client_type: "claude",
      runtime_instance_id: "rtinst_1",
      internal_event_url: "http://localhost/internal/v1/events",
    } } }), { status: 200 }));
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ hook_event_name: "Stop", last_assistant_message: undefined }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.completed"]);
  });

  test("StopFailure resolves current turn and reports failed", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { current_turn: {
      session_id: "sess_1",
      turn_id: "turn_1",
      client_type: "claude",
      runtime_instance_id: "rtinst_1",
      internal_event_url: "http://localhost/internal/v1/events",
    } } }), { status: 200 }));
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ hook_event_name: "StopFailure", error: "rate_limited", error_details: "try later" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.failed"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      data: { failure: { message: "rate_limited: try later" } },
    });
  });

  test("manual UserPromptSubmit reuses the runtime context established by SessionStart", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      expect(url).toBe("http://localhost/internal/v1/agent-bindings/session-context?client_type=claude&client_session_key=claude_session_1");
      return new Response(JSON.stringify({ data: { session_context: {
        session_id: "sess_manual",
        client_type: "claude",
        client_session_key: "claude_session_1",
        runtime_instance_id: "rtinst_stable",
        internal_event_url: "http://localhost/internal/v1/events",
      } } }), { status: 200 });
    });
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ hook_event_name: "UserPromptSubmit", prompt: "typed manually", cwd: workspace }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_manual",
      data: { runtime_instance_id: "rtinst_stable", input: { summary: "typed manually" } },
    });
  });

  test("SessionEnd reports exited using managed runtime env", async () => {
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_1", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
    });

    await runClaudeHook(baseInput({ hook_event_name: "SessionEnd", reason: "quit" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["session.exited"]);
    expect(reported[0]).toEqual({
      session_id: "sess_1",
      type: "session.exited",
      data: { reason: "quit", runtime_instance_id: "rtinst_1" },
    });
  });

  test("manual SessionEnd resolves its stable session context and reports exited", async () => {
    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://localhost/internal/v1/agent-bindings/session-context?client_type=claude&client_session_key=claude_session_1");
      return new Response(JSON.stringify({ data: { session_context: {
        session_id: "sess_manual",
        client_type: "claude",
        client_session_key: "claude_session_1",
        runtime_instance_id: "rtinst_stable",
        internal_event_url: "http://localhost/internal/v1/events",
      } } }), { status: 200 });
    });
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ hook_event_name: "SessionEnd", reason: "prompt_input_exit" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["session.exited"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_manual",
      data: { runtime_instance_id: "rtinst_stable", reason: "prompt_input_exit" },
    });
  });

  test("MessageDisplay is ignored in phase 2", async () => {
    const { deps, reported } = install();

    await runClaudeHook(baseInput({ hook_event_name: "MessageDisplay", delta: "partial", final: false }), deps);

    expect(reported).toEqual([]);
  });
});
