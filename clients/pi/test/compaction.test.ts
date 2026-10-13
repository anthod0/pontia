import { realpath } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import type { CompactionControl, LifecycleControl } from "../src/control-socket.js";
import type { InternalEvent } from "../src/events.js";
import { tempDir } from "./temp-dir.js";

async function fixture() {
  const root = await tempDir("pc-");
  const workspace = await realpath(root);
  const events: InternalEvent[] = [];
  const controls: CompactionControl[] = [];
  const lifecycle: LifecycleControl[] = [];
  let existing = false;
  let native = "native";
  let idle = true;
  const context = {
    mode: "tui",
    isIdle: () => idle,
    hasPendingMessages: () => false,
    compact: vi.fn<(options: any) => void>(),
    abort: vi.fn(),
    shutdown: vi.fn(),
    sessionManager: {
      getSessionId: () => native,
      getSessionFile: () => join(root, "native.jsonl"),
      getCwd: () => workspace,
    },
  };
  const handlers: Record<string, (event: any, ctx?: any) => Promise<void>> = {};
  const install = async (reason = "startup") => {
    createPontiaPiExtension(
      {
        on(name: string, handler: any) {
          handlers[name] = handler;
        },
        registerCommand() {},
      } as any,
      {
        env: { PONTIA_HOME: root, TMUX: "/unused,1,1", TMUX_PANE: "%1" },
        isManagedPane: async () => true,
        logDiagnostic: async () => {},
        connectPi: async (
          _home,
          _error,
          _submit,
          _models,
          _replay,
          control,
          _history,
          compaction,
        ) => {
          controls.push(compaction!);
          lifecycle.push(control!);
          return {
            close: async () => {},
            registered() {},
            async request(method, params) {
              if (method === "workspaces.list")
                return { workspaces: [{ canonical_path: workspace, state: "active" }] };
              if (method === "session.context")
                return {
                  session_context: existing
                    ? {
                        session_id: "sess",
                        runtime_id: "runtime",
                        client_type: "pi",
                        session_state: "idle",
                      }
                    : null,
                };
              if (method === "runtime.register") {
                existing = true;
                return { session: { session_id: "sess" }, runtime: { runtime_id: "runtime" } };
              }
              if (method === "event.report") {
                events.push((params as any).event);
                return { accepted: true };
              }
              throw new Error(`Unexpected method ${method}`);
            },
          };
        },
      },
    );
    await handlers.session_start({ reason }, context);
    events.length = 0;
  };
  await install();
  return {
    events,
    handlers,
    context,
    controls,
    lifecycle,
    install,
    setIdle(value: boolean) {
      idle = value;
    },
    setNative(value: string) {
      native = value;
    },
  };
}

const before = (reason: string) => ({ reason, signal: new AbortController().signal });
const completed = (reason: string, id = "compact-native") => ({
  reason,
  compactionEntry: { id, tokensBefore: 20000, summary: "private native summary" },
});

test("manual native compaction correlates hooks and callbacks with the accepted operation", async () => {
  const f = await fixture();
  f.controls[0].compact("compact-api");
  expect(f.context.compact).toHaveBeenCalledOnce();
  expect(f.events).toEqual([]); // the control receipt is not an outcome fact
  await f.handlers.session_before_compact(before("manual"));
  await f.handlers.session_compact(completed("manual"));
  const options = f.context.compact.mock.calls[0][0];
  options.onComplete({
    summary: "private native summary",
    tokensBefore: 20000,
    estimatedTokensAfter: 3200,
  });
  await vi.waitFor(() => expect(f.events).toHaveLength(2));
  expect(f.events.map((event) => event.type)).toEqual([
    "session.compaction_started",
    "session.compaction_completed",
  ]);
  expect(f.events.map((event) => event.data.operation_id)).toEqual(["compact-api", "compact-api"]);
  expect(f.events[1].data).toEqual({
    operation_id: "compact-api",
    trigger: "manual",
    native_compaction_id: "compact-native",
    tokens_before: 20000,
    tokens_after: 3200,
  });
  expect(f.events.every((event) => event.turn_id === undefined)).toBe(true);
  expect(JSON.stringify(f.events)).not.toContain("private native summary");
  f.controls[0].compact("compact-api");
  options.onComplete({ tokensBefore: 20000 });
  await f.handlers.session_compact(completed("manual"));
  expect(f.context.compact).toHaveBeenCalledOnce();
  expect(f.events).toHaveLength(2);
});

test.each(["manual", "threshold", "overflow"])(
  "native %s hooks keep one lifecycle across reload",
  async (reason) => {
    const f = await fixture();
    const start = before(reason);
    await f.handlers.session_before_compact(start);
    await f.handlers.session_before_compact(start);
    const operation = f.events[0].data.operation_id;
    expect(f.events).toHaveLength(1);
    await f.handlers.session_shutdown({ reason: "reload" });
    await f.install("reload");
    await f.handlers.session_before_compact(start);
    await f.handlers.session_compact(completed(reason));
    await f.handlers.session_compact(completed(reason));
    expect(f.events).toHaveLength(1);
    expect(f.events[0].type).toBe("session.compaction_completed");
    expect(f.events[0].data).toEqual({
      operation_id: operation,
      trigger: reason,
      native_compaction_id: "compact-native",
      tokens_before: 20000,
      tokens_after: null,
    });
    await f.handlers.session_before_compact(before(reason));
    await f.handlers.session_compact(completed(reason, "second-native"));
    expect(f.events).toHaveLength(3);
    expect(f.events[1].data.operation_id).not.toBe(operation);
    expect(f.events[1].data.operation_id).toBe(f.events[2].data.operation_id);
  },
);

test("a manual callback after extension reload reports through the current binding", async () => {
  const f = await fixture();
  f.controls[0].compact("reload-manual");
  await f.handlers.session_before_compact(before("manual"));
  const options = f.context.compact.mock.calls[0][0];
  await f.handlers.session_shutdown({ reason: "reload" });
  await f.install("reload");
  await f.handlers.session_compact(completed("manual"));
  options.onComplete({ tokensBefore: 20000, estimatedTokensAfter: 3200 });
  await vi.waitFor(() => expect(f.events).toHaveLength(1));
  expect(f.events[0].type).toBe("session.compaction_completed");
  expect(f.events[0].data.operation_id).toBe("reload-manual");
});

test.each([false, true])(
  "native failure preserves cancellation=%s and deduplicates the callback",
  async (aborted) => {
    const f = await fixture();
    f.controls[0].compact("compact-failed");
    await f.handlers.session_before_compact(before("manual"));
    const failed = {
      reason: "manual",
      aborted,
      errorMessage: aborted ? undefined : "native provider error",
    };
    await f.handlers.session_compact_failed(failed);
    await f.handlers.session_compact_failed({ ...failed });
    f.context.compact.mock.calls[0][0].onError(new Error("callback error"));
    await vi.waitFor(() => expect(f.events).toHaveLength(2));
    expect(f.events[1].data).toEqual({
      operation_id: "compact-failed",
      trigger: "manual",
      aborted,
      error_message: aborted ? null : "native provider error",
    });
    f.controls[0].compact("next-attempt");
    await f.handlers.session_before_compact(before("manual"));
    await f.handlers.session_compact_failed({ ...failed });
    expect(f.events).toHaveLength(4);
  },
);

test("a native rejection before preparation still terminates the accepted manual request", async () => {
  const f = await fixture();
  f.controls[0].compact("too-small");
  await f.handlers.session_compact_failed({
    reason: "manual",
    aborted: false,
    errorMessage: "Nothing to compact",
  });
  f.context.compact.mock.calls[0][0].onError(new Error("Nothing to compact"));
  await vi.waitFor(() => expect(f.events).toHaveLength(2));
  expect(f.events.map((event) => event.data.operation_id)).toEqual(["too-small", "too-small"]);
  expect(f.events[1].type).toBe("session.compaction_failed");
});

test("overflow recovery failure after a completed compaction has its own lifecycle", async () => {
  const f = await fixture();
  await f.handlers.session_before_compact(before("overflow"));
  await f.handlers.session_compact(completed("overflow"));
  const failed = { reason: "overflow", aborted: false, errorMessage: "retry still overflowed" };
  await f.handlers.session_compact_failed(failed);
  await f.handlers.session_compact_failed({ ...failed });
  expect(f.events.map((event) => event.type)).toEqual([
    "session.compaction_started",
    "session.compaction_completed",
    "session.compaction_started",
    "session.compaction_failed",
  ]);
  expect(f.events[2].data.operation_id).not.toBe(f.events[0].data.operation_id);
  expect(f.events[2].data.operation_id).toBe(f.events[3].data.operation_id);
  expect(f.events[3].data.error_message).toBe("retry still overflowed");
});

test("busy, stale Runtime and changed native Session controls never call compact or abort", async () => {
  const f = await fixture();
  f.setIdle(false);
  expect(() => f.controls[0].compact("busy")).toThrow();
  f.setIdle(true);
  f.setNative("changed");
  expect(() => f.controls[0].compact("changed")).toThrow();
  f.setNative("native");
  await f.handlers.session_shutdown({ reason: "reload" });
  await f.install("reload");
  expect(() => f.controls[0].compact("stale")).toThrow();
  expect(f.context.compact).not.toHaveBeenCalled();
  expect(f.context.abort).not.toHaveBeenCalled();
});
