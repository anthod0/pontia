import { realpath, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { runClaudeHook } from "../src/hook.js";
import { tempDir as isolatedTempDir } from "./temp-dir.js";

let defaultPontiaHome;

async function tempDir() {
    return isolatedTempDir("pontia-claude-hook-");
}

beforeEach(async () => {
    defaultPontiaHome = await isolatedTempDir("pontia-claude-hook-home-");
    await writeFile(join(defaultPontiaHome, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');
});

afterEach(() => {
    vi.restoreAllMocks();
});
function baseInput(overrides = {}) {
    return {
        hook_event_name: "SessionStart",
        session_id: "claude_session_1",
        transcript_path: "/tmp/claude/session.jsonl",
        cwd: "/repo",
        ...overrides,
    };
}
function install(overrides = {}) {
    const reported = [];
    const diagnostics = [];
    const env = {
        PONTIA_HOME: defaultPontiaHome,
        TMUX: "/tmp/tmux-1000/default,2071,502",
        TMUX_PANE: "%42",
        ...(overrides.env ?? {}),
    };
    const { env: _ignoredEnv, managedRuntime, keyContext = managedRuntime, fetch: suppliedFetch, ...dependencies } = overrides;
    const fetchImpl = suppliedFetch && keyContext
        ? vi.fn(async (url, init) => {
            if (String(url).includes("/internal/v1/agent-bindings/session-context?")) {
                return new Response(JSON.stringify({ data: { session_context: {
                    session_id: keyContext.sessionId,
                    session_state: "idle",
                    client_type: "claude",
                    runtime_instance_id: keyContext.runtimeInstanceId,
                    internal_event_url: "http://localhost/internal/v1/events",
                } } }), { status: 200 });
            }
            return suppliedFetch(url, init);
        })
        : suppliedFetch;
    return {
        reported,
        diagnostics,
        deps: {
            env,
            makeReporter: vi.fn(() => ({ report: vi.fn(async (_ctx, event) => {
                    reported.push(event);
                    return {
                        accepted: true,
                        turnId: event.type === "turn.started" ? "turn_canonical" : event.turn_id,
                    };
                }) })),
            logDiagnostic: vi.fn(async (_logFile, entry) => {
                diagnostics.push(entry);
            }),
            loadManagedRuntime: vi.fn(async () => managedRuntime),
            fetch: fetchImpl,
            ...dependencies,
        },
    };
}
describe("pontia claude hook", () => {
    test("hooks are a silent no-op outside tmux", async () => {
        const fetchImpl = vi.fn();
        const loadManagedRuntime = vi.fn();
        const { deps, reported } = install({
            env: { TMUX: undefined, TMUX_PANE: undefined },
            fetch: fetchImpl,
            loadManagedRuntime,
        });

        const output = await runClaudeHook(baseInput(), deps);

        expect(output).toBeUndefined();
        expect(loadManagedRuntime).not.toHaveBeenCalled();
        expect(fetchImpl).not.toHaveBeenCalled();
        expect(reported).toEqual([]);
    });
    test("unknown client keys do not report lifecycle events", async () => {
        const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 }));
        const { deps, reported } = install({
            fetch: fetchImpl,
            isManagedPane: vi.fn(async () => false),
        });

        await runClaudeHook(baseInput({ hook_event_name: "UserPromptSubmit", prompt: "ignored" }), deps);
        await runClaudeHook(baseInput({ hook_event_name: "Stop", last_assistant_message: "ignored" }), deps);
        await runClaudeHook(baseInput({ hook_event_name: "SessionEnd" }), deps);

        expect(fetchImpl).toHaveBeenCalled();
        expect(reported).toEqual([]);
    });
    test("SessionStart does not use stale tmux markers as the Claude session identity", async () => {
        const workspace = await realpath(await tempDir());
        const fetchImpl = vi.fn(async (url, init) => {
            if (url === "http://localhost/external/v1/workspaces") {
                return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
            }
            if (url.startsWith("http://localhost/internal/v1/agent-bindings/session-context?")) {
                return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
            }
            expect(url).toBe("http://localhost/internal/v1/runtime-bindings/upsert");
            const request = JSON.parse(String(init?.body));
            expect(request.client_session_key).toBe("claude_session_1");
            expect(request).not.toHaveProperty("session_id");
            expect(request).not.toHaveProperty("runtime_instance_id");
            return new Response(JSON.stringify({
                session: { session_id: "sess_fresh" },
                runtime: { runtime_instance_id: "rtinst_fresh", internal_event_url: "http://localhost/internal/v1/events" },
            }), { status: 200 });
        });
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_stale", runtimeInstanceId: "rtinst_stale" },
            keyContext: null,
            fetch: fetchImpl,
        });
        await runClaudeHook(baseInput({ cwd: workspace }), deps);
        expect(reported[0]).toMatchObject({
            session_id: "sess_fresh",
            data: { runtime_instance_id: "rtinst_fresh", client_session_key: "claude_session_1" },
        });
    });
    test.each(["idle", "busy", "interrupted"])("SessionStart suppresses a duplicate TUI for a key bound to a %s Session", async (sessionState) => {
        const workspace = await realpath(await tempDir());
        const fetchImpl = vi.fn(async (url) => {
            if (url === "http://localhost/external/v1/workspaces") {
                return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
            }
            if (url.startsWith("http://localhost/internal/v1/agent-bindings/session-context?")) {
                return new Response(JSON.stringify({ data: { session_context: {
                    session_id: "sess_active",
                    session_state: sessionState,
                    client_type: "claude",
                    runtime_instance_id: "rtinst_active",
                    internal_event_url: "http://localhost/internal/v1/events",
                } } }), { status: 200 });
            }
            return new Response("unexpected", { status: 500 });
        });
        const { deps, reported, diagnostics } = install({ fetch: fetchImpl });
        await runClaudeHook(baseInput({ cwd: workspace }), deps);
        expect(reported).toEqual([]);
        expect(diagnostics).toEqual([expect.objectContaining({ code: "duplicate_active_client_session" })]);
        expect(fetchImpl).toHaveBeenCalledTimes(2);
    });
    test("SessionStart finishes binding a starting Session and reports ready", async () => {
        const workspace = await realpath(await tempDir());
        const fetchImpl = vi.fn(async (url, init) => {
            if (url === "http://localhost/external/v1/workspaces") {
                return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
            }
            if (url.startsWith("http://localhost/internal/v1/agent-bindings/session-context?")) {
                return new Response(JSON.stringify({ data: { session_context: {
                    session_id: "sess_starting",
                    session_state: "starting",
                    client_type: "claude",
                    runtime_instance_id: "rtinst_starting",
                    internal_event_url: "http://localhost/internal/v1/events",
                } } }), { status: 200 });
            }
            if (url === "http://localhost/internal/v1/runtime-bindings/upsert") {
                expect(JSON.parse(String(init?.body))).toMatchObject({
                    client_session_key: "claude_session_1",
                    runtime_instance_id: "rtinst_starting",
                });
                return new Response(JSON.stringify({
                    session: { session_id: "sess_starting" },
                    runtime: { runtime_instance_id: "rtinst_bound", internal_event_url: "http://localhost/internal/v1/events" },
                }), { status: 200 });
            }
            return new Response("unexpected", { status: 500 });
        });
        const { deps, reported, diagnostics } = install({ fetch: fetchImpl });

        await runClaudeHook(baseInput({ cwd: workspace }), deps);

        expect(fetchImpl).toHaveBeenCalledTimes(3);
        expect(diagnostics).toEqual([]);
        expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
        expect(reported[0]).toMatchObject({
            session_id: "sess_starting",
            data: { runtime_instance_id: "rtinst_bound" },
        });
    });
    test("manual SessionStart creates a binding in an active workspace and reports ready", async () => {
        const workspace = await realpath(await tempDir());
        const fetchImpl = vi.fn(async (url, init) => {
            if (url === "http://localhost/external/v1/workspaces") {
                return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
            }
            if (url.startsWith("http://localhost/internal/v1/agent-bindings/session-context?")) {
                return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
            }
            expect(url).toBe("http://localhost/internal/v1/runtime-bindings/upsert");
            expect(init?.method).toBe("POST");
            const request = JSON.parse(String(init?.body));
            expect(request).toMatchObject({
                client_type: "claude",
                client_session_key: "claude_session_1",
                client_session_file: "/tmp/claude/session.jsonl",
                client_cwd: workspace,
            });
            expect(request).not.toHaveProperty("metadata");
            expect(request).not.toHaveProperty("runtime_instance_id");
            return new Response(JSON.stringify({
                session: { session_id: "sess_manual" },
                runtime: { runtime_instance_id: "rtinst_manual", internal_event_url: "http://localhost/internal/v1/events" },
            }), { status: 200 });
        });
        const { deps, reported } = install({ fetch: fetchImpl });
        await runClaudeHook(baseInput({ cwd: workspace }), deps);
        expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
        expect(reported[0]).toMatchObject({
            session_id: "sess_manual",
            data: { client_session_key: "claude_session_1", runtime_instance_id: "rtinst_manual" },
        });
    });
    test("manual SessionStart no-ops when workspace is not active", async () => {
        const workspace = await realpath(await tempDir());
        const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 }));
        const { deps, reported, diagnostics } = install({ fetch: fetchImpl });
        await runClaudeHook(baseInput({ cwd: workspace }), deps);
        expect(reported).toEqual([]);
        expect(diagnostics).toEqual([expect.objectContaining({ code: "workspace_not_active" })]);
    });
    test("UserPromptSubmit claims managed pending turn and reports turn.started", async () => {
        const fetchImpl = vi.fn(async (url, init) => {
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
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
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
        const fetchImpl = vi.fn(async (url, init) => {
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
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
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
        const fetchImpl = vi.fn(async (url, init) => {
            expect(url).toBe("http://localhost/internal/v1/sessions/sess_1/current-turn/claim");
            expect(JSON.parse(String(init?.body))).toMatchObject({ runtime_instance_id: "rtinst_1", client_type: "claude" });
            return new Response(JSON.stringify({ data: { current_turn: null } }), { status: 200 });
        });
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
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
        const fetchImpl = vi.fn(async (url) => {
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
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
        });
        await runClaudeHook(baseInput({ hook_event_name: "Stop", last_assistant_message: "done" }), deps);
        expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
        expect(reported[0]).toMatchObject({
            session_id: "sess_1",
            turn_id: "turn_1",
            data: { output: { summary: "done" } },
        });
    });
    test("Stop rejects a current turn that disagrees with the tmux runtime identity", async () => {
        const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { current_turn: {
                    session_id: "sess_other",
                    turn_id: "turn_1",
                    client_type: "claude",
                    runtime_instance_id: "rtinst_other",
                    internal_event_url: "http://localhost/internal/v1/events",
                } } }), { status: 200 }));
        const { deps, reported, diagnostics } = install({
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
        });

        await runClaudeHook(baseInput({ hook_event_name: "Stop", last_assistant_message: "ignored" }), deps);

        expect(reported).toEqual([]);
        expect(diagnostics).toEqual([expect.objectContaining({
            code: "current_turn_lookup_failed",
            message: "current turn context does not match tmux runtime identity",
        })]);
    });
    test("Stop does not infer output from the transcript when last_assistant_message is missing", async () => {
        const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { current_turn: {
                    session_id: "sess_1",
                    turn_id: "turn_1",
                    client_type: "claude",
                    runtime_instance_id: "rtinst_1",
                    internal_event_url: "http://localhost/internal/v1/events",
                } } }), { status: 200 }));
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
        });
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
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_1", runtimeInstanceId: "rtinst_1" },
            fetch: fetchImpl,
        });
        await runClaudeHook(baseInput({ hook_event_name: "StopFailure", error: "rate_limited", error_details: "try later" }), deps);
        expect(reported.map((event) => event.type)).toEqual(["turn.failed"]);
        expect(reported[0]).toMatchObject({
            session_id: "sess_1",
            turn_id: "turn_1",
            data: { failure: { message: "rate_limited: try later" } },
        });
    });
    test("PermissionRequest registers the native request and stays silent when resolved elsewhere", async () => {
        const hookInput = baseInput({
            hook_event_name: "PermissionRequest",
            prompt_id: "prompt_1",
            tool_name: "Bash",
            tool_input: { command: "pnpm test", description: "run tests" },
            permission_suggestions: [{
                    type: "addRules",
                    rules: [{ toolName: "Bash", ruleContent: "pnpm test" }],
                    behavior: "allow",
                    destination: "localSettings",
                }],
        });
        const fetchImpl = vi.fn(async (url, init) => {
            expect(url).toBe("http://localhost/internal/v1/claude/permission-request");
            expect(init?.method).toBe("POST");
            expect(JSON.parse(String(init?.body))).toEqual({
                session_id: "claude_session_1",
                prompt_id: "prompt_1",
                tool_name: "Bash",
                tool_input: { command: "pnpm test", description: "run tests" },
                permission_suggestions: [{
                        type: "addRules",
                        rules: [{ toolName: "Bash", ruleContent: "pnpm test" }],
                        behavior: "allow",
                        destination: "localSettings",
                    }],
                hook_input: hookInput,
            });
            return new Response(JSON.stringify({
                data: { result: "resolved_elsewhere", request_event_id: "evt_request" },
            }), { status: 200 });
        });
        const { deps, reported, diagnostics } = install({ fetch: fetchImpl });
        const output = await runClaudeHook(hookInput, deps);
        expect(output).toBeUndefined();
        expect(reported).toEqual([]);
        expect(diagnostics).toEqual([]);
    });
    test("PermissionRequest no-ops with empty stdout when Pontia has no binding or active Turn", async () => {
        const fetchImpl = vi.fn(async () => new Response(null, { status: 204 }));
        const { deps, diagnostics } = install({ fetch: fetchImpl });
        const output = await runClaudeHook(baseInput({
            hook_event_name: "PermissionRequest",
            tool_name: "Write",
            tool_input: { file_path: "/repo/file" },
        }), deps);
        expect(output).toBeUndefined();
        expect(diagnostics).toEqual([]);
    });
    test.each(["managed", "manual"])("SessionStart and PermissionRequest use the same Approval path for %s sessions", async (kind) => {
        const workspace = await realpath(await tempDir());
        const managedRuntime = kind === "managed"
            ? { sessionId: "sess_managed", runtimeInstanceId: "rtinst_managed" }
            : undefined;
        const fetchImpl = vi.fn(async (url, init) => {
            if (url === "http://localhost/external/v1/workspaces") {
                return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
            }
            if (url.startsWith("http://localhost/internal/v1/agent-bindings/session-context?")) {
                return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
            }
            if (url === "http://localhost/internal/v1/runtime-bindings/upsert") {
                const request = JSON.parse(String(init?.body));
                return new Response(JSON.stringify({
                    session: { session_id: "sess_manual" },
                    runtime: {
                        runtime_instance_id: "rtinst_manual",
                        internal_event_url: "http://localhost/internal/v1/events",
                    },
                }), { status: 200 });
            }
            expect(url).toBe("http://localhost/internal/v1/claude/permission-request");
            expect(JSON.parse(String(init?.body))).toMatchObject({
                session_id: "claude_session_1",
                prompt_id: "prompt_1",
                tool_name: "Bash",
            });
            return new Response(JSON.stringify({
                data: { result: { decision: "accept_once" }, request_event_id: "evt_request" },
            }), { status: 200 });
        });
        const { deps, reported } = install({ managedRuntime, fetch: fetchImpl });
        await runClaudeHook(baseInput({ cwd: workspace }), deps);
        const output = await runClaudeHook(baseInput({
            hook_event_name: "PermissionRequest",
            prompt_id: "prompt_1",
            tool_name: "Bash",
            tool_input: { command: "pnpm test" },
            cwd: workspace,
        }), deps);
        expect(reported.map((event) => event.type)).toEqual(kind === "managed" ? [] : ["session.ready"]);
        expect(output).toEqual({
            hookSpecificOutput: {
                hookEventName: "PermissionRequest",
                decision: { behavior: "allow" },
            },
        });
        expect(fetchImpl.mock.calls.filter(([url]) => url === "http://localhost/internal/v1/claude/permission-request")).toHaveLength(1);
    });
    test.each([
        [
            "accept once",
            { decision: "accept_once" },
            {
                hookSpecificOutput: {
                    hookEventName: "PermissionRequest",
                    decision: { behavior: "allow" },
                },
            },
        ],
        [
            "reject",
            { decision: "reject" },
            {
                hookSpecificOutput: {
                    hookEventName: "PermissionRequest",
                    decision: { behavior: "deny" },
                },
            },
        ],
        [
            "always allow",
            {
                decision: "always_allow",
                permission_suggestion: {
                    type: "addRules",
                    rules: [{ toolName: "Bash", ruleContent: "pnpm test" }],
                    behavior: "allow",
                    destination: "localSettings",
                },
            },
            {
                hookSpecificOutput: {
                    hookEventName: "PermissionRequest",
                    decision: {
                        behavior: "allow",
                        updatedPermissions: [{
                            type: "addRules",
                            rules: [{ toolName: "Bash", ruleContent: "pnpm test" }],
                            behavior: "allow",
                            destination: "localSettings",
                        }],
                    },
                },
            },
        ],
    ])("PermissionRequest emits Claude's exact structured response for %s", async (_label, result, expected) => {
        const fetchImpl = vi.fn(async () => new Response(JSON.stringify({
            data: { result, request_event_id: "evt_request" },
        }), { status: 200 }));
        const { deps, diagnostics } = install({ fetch: fetchImpl });
        const output = await runClaudeHook(baseInput({
            hook_event_name: "PermissionRequest",
            tool_name: "Bash",
            tool_input: { command: "pnpm test" },
        }), deps);
        expect(output).toEqual(expected);
        expect(diagnostics).toEqual([]);
    });
    test("PermissionRequest never emits malformed output for HTTP or response failures", async () => {
        for (const response of [
            new Response("upstream failed", { status: 500 }),
            new Response("{malformed", { status: 200 }),
            new Response(JSON.stringify({ data: { result: { decision: "always_allow" } } }), { status: 200 }),
        ]) {
            const fetchImpl = vi.fn(async () => response);
            const { deps } = install({ fetch: fetchImpl });
            const output = await runClaudeHook(baseInput({
                hook_event_name: "PermissionRequest",
                tool_name: "Bash",
                tool_input: { command: "pnpm test" },
            }), deps);
            expect(output).toBeUndefined();
        }
    });
    test("manual UserPromptSubmit recovers the runtime context established by SessionStart from tmux", async () => {
        const fetchImpl = vi.fn(async (url) => {
            expect(url).toBe("http://localhost/internal/v1/sessions/sess_manual/current-turn/claim");
            return new Response(JSON.stringify({ data: { current_turn: null } }), { status: 200 });
        });
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_manual", runtimeInstanceId: "rtinst_stable" },
            fetch: fetchImpl,
        });
        await runClaudeHook(baseInput({ hook_event_name: "UserPromptSubmit", prompt: "typed manually" }), deps);
        expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
        expect(reported[0]).toMatchObject({
            session_id: "sess_manual",
            data: { runtime_instance_id: "rtinst_stable", input: { summary: "typed manually" } },
        });
    });
    test("SessionEnd resolves the Session by client key without tmux markers", async () => {
        const fetchImpl = vi.fn(async (url) => {
            expect(url).toContain("/internal/v1/agent-bindings/session-context?client_type=claude&client_session_key=claude_session_1");
            return new Response(JSON.stringify({ data: { session_context: {
                session_id: "sess_1",
                session_state: "idle",
                client_type: "claude",
                runtime_instance_id: "rtinst_1",
                internal_event_url: "http://localhost/internal/v1/events",
            } } }), { status: 200 });
        });
        const { deps, reported } = install({ fetch: fetchImpl, managedRuntime: undefined });
        await runClaudeHook(baseInput({ hook_event_name: "SessionEnd", reason: "quit" }), deps);
        expect(reported.map((event) => event.type)).toEqual(["session.exited"]);
        expect(reported[0]).toEqual({
            session_id: "sess_1",
            type: "session.exited",
            data: { reason: "quit", runtime_instance_id: "rtinst_1" },
        });
    });
    test("SessionEnd does not fall back to stale tmux markers when key lookup fails", async () => {
        const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 }));
        const { deps, reported } = install({
            managedRuntime: { sessionId: "sess_stale", runtimeInstanceId: "rtinst_stale" },
            keyContext: null,
            fetch: fetchImpl,
        });
        await runClaudeHook(baseInput({ hook_event_name: "SessionEnd", reason: "prompt_input_exit" }), deps);
        expect(reported).toEqual([]);
    });
    test("MessageDisplay is ignored in phase 2", async () => {
        const { deps, reported } = install();
        await runClaudeHook(baseInput({ hook_event_name: "MessageDisplay", delta: "partial", final: false }), deps);
        expect(reported).toEqual([]);
    });
});
