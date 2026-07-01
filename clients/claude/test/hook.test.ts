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
        return true;
      }) })),
      logDiagnostic: vi.fn(async (_logFile: string, entry: unknown) => {
        diagnostics.push(entry);
      }),
      ...overrides,
    },
  };
}

describe("pontia claude hook", () => {
  test("SessionStart reports ready from managed runtime env after active workspace check", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://localhost/external/v1/workspaces");
      return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
    });
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_ready", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
      fetch: fetchImpl as any,
    });

    await runClaudeHook(baseInput({ cwd: workspace }), deps);

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_ready",
      turn_id: null,
      source: "agent_client",
      client_type: "claude",
      payload: {
        runtime_instance_id: "rtinst_1",
        client_session_key: "claude_session_1",
        transcript_path: "/tmp/claude/session.jsonl",
        client_cwd: workspace,
      },
    });
  });

  test("SessionStart no-ops when workspace is not active", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 }));
    const { deps, reported, diagnostics } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_ready", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
      fetch: fetchImpl as any,
    });

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
    expect(reported[0]).toMatchObject({
      session_id: "sess_1",
      turn_id: "turn_1",
      source: "agent_adapter",
      client_type: "claude",
      payload: { runtime_instance_id: "rtinst_1", input: { summary: "build it" } },
    });
  });

  test("UserPromptSubmit accepts managed pending turn without backend turn_id and reports generated turn.started", async () => {
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
    expect(reported[0]).toMatchObject({
      session_id: "sess_1",
      source: "agent_adapter",
      client_type: "claude",
      payload: { runtime_instance_id: "rtinst_1", input: { summary: "build it" } },
    });
    expect(reported[0]?.turn_id).toMatch(/^turn_/);
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
    expect(reported[0]).toMatchObject({
      session_id: "sess_1",
      source: "agent_adapter",
      client_type: "claude",
      payload: { runtime_instance_id: "rtinst_1", input: { summary: "typed in tui" } },
    });
    expect(reported[0]?.turn_id).toMatch(/^turn_/);
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
      payload: { output: { summary: "done" } },
    });
  });

  test("Stop falls back to latest transcript assistant text when last_assistant_message is missing", async () => {
    const dir = await tempDir();
    const transcriptPath = join(dir, "session.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({ type: "user", message: { role: "user", content: "hello" } }),
      JSON.stringify({ type: "assistant", message: { role: "assistant", content: [{ type: "thinking", thinking: "plan" }] } }),
      JSON.stringify({ type: "assistant", message: { role: "assistant", content: [{ type: "text", text: "Hi there" }] } }),
      JSON.stringify({ type: "assistant", message: { role: "assistant", content: [{ type: "redacted_thinking", data: "opaque" }] } }),
      "",
    ].join("\n"));
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { current_turn: {
      session_id: "sess_1",
      turn_id: "turn_1",
      client_type: "claude",
      runtime_instance_id: "rtinst_1",
      internal_event_url: "http://localhost/internal/v1/events",
    } } }), { status: 200 }));
    const { deps, reported } = install({ fetch: fetchImpl as any });

    await runClaudeHook(baseInput({ hook_event_name: "Stop", transcript_path: transcriptPath }), deps);

    expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
    expect(reported[0]).toMatchObject({ payload: { output: { summary: "Hi there" } } });
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
      payload: { failure: { message: "rate_limited: try later" } },
    });
  });

  test("SessionEnd reports exited using managed runtime env", async () => {
    const { deps, reported } = install({
      env: { PONTIA_HOME: defaultPontiaHome, PONTIA_SESSION_ID: "sess_1", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1" },
    });

    await runClaudeHook(baseInput({ hook_event_name: "SessionEnd", reason: "quit" }), deps);

    expect(reported.map((event) => event.type)).toEqual(["session.exited"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_1",
      turn_id: null,
      source: "agent_client",
      client_type: "claude",
      payload: { reason: "quit", runtime_instance_id: "rtinst_1" },
    });
  });

  test("MessageDisplay is ignored in phase 2", async () => {
    const { deps, reported } = install();

    await runClaudeHook(baseInput({ hook_event_name: "MessageDisplay", delta: "partial", final: false }), deps);

    expect(reported).toEqual([]);
  });
});
