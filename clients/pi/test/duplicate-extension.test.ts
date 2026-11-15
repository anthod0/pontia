import { execFileSync } from "node:child_process";
import { realpath } from "node:fs/promises";
import { join } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { tempDir } from "./temp-dir.js";

const { connect, diagnostic } = vi.hoisted(() => ({
  connect: vi.fn(),
  diagnostic: vi.fn(async () => {}),
}));
vi.mock("../src/control-socket.js", async (original) => ({
  ...(await original<typeof import("../src/control-socket.js")>()),
  connectPi: connect,
}));
vi.mock("../src/managed-runtime.js", async (original) => ({
  ...(await original<typeof import("../src/managed-runtime.js")>()),
  isPontiaManagedTmuxPane: async () => true,
}));
vi.mock("../src/diagnostics.js", () => ({ appendDiagnostic: diagnostic }));

type Handler = (event: any, context: any) => unknown;
const extensions: Array<{ emit(name: string, reason: string, session?: string): Promise<void> }> =
  [];
let root: string;

function fakePi() {
  const handlers = new Map<string, Handler[]>();
  const pi = {
    on(name: string, handler: Handler) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    registerCommand: vi.fn(),
  };
  const extension = {
    pi: pi as unknown as ExtensionAPI,
    handlers,
    async emit(name: string, reason: string, session = "one") {
      for (const handler of handlers.get(name) ?? []) {
        await handler(
          { type: name, reason },
          {
            mode: "tui",
            sessionManager: {
              getSessionId: () => session,
              getSessionFile: () => join(root, `${session}.jsonl`),
              getCwd: () => root,
            },
          },
        );
      }
    },
  };
  extensions.push(extension);
  return extension;
}

async function load() {
  // Separate module instances model the local and npm copies of the plugin.
  vi.resetModules();
  return (await import("../src/index.js")).default;
}

function childSession() {
  return execFileSync("sh", ["-c", 'printf %s "${PONTIA_SESSION_ID-unset}"'], {
    encoding: "utf8",
  });
}

beforeEach(async () => {
  root = await realpath(await tempDir("pi-duplicate-"));
  vi.stubEnv("PONTIA_HOME", root);
  vi.stubEnv("PONTIA_SESSION_ID", "stale");
  vi.stubEnv("TMUX", "/unused/tmux,1,1");
  vi.stubEnv("TMUX_PANE", "%1");
  diagnostic.mockClear();
  const sessions = new Set<string>();
  connect.mockReset().mockImplementation(async () => ({
    registered() {},
    async close() {},
    async request(method: string, params: any) {
      if (method === "workspaces.list")
        return { workspaces: [{ canonical_path: root, state: "active" }] };
      if (method === "session.context") {
        const id = params.client_session_key;
        return {
          session_context: sessions.has(id)
            ? {
                session_id: `sess_${id}`,
                runtime_id: `rt_${id}`,
                client_type: "pi",
                session_state: "idle",
              }
            : null,
        };
      }
      if (method === "runtime.register") {
        const id = params.binding.client_session_key;
        sessions.add(id);
        return { session: { session_id: `sess_${id}` }, runtime: { runtime_id: `rt_${id}` } };
      }
      if (method === "event.report") return { accepted: true };
      throw new Error(`Unexpected RPC: ${method}`);
    },
  }));
});

afterEach(async () => {
  try {
    for (const extension of extensions.splice(0)) {
      await extension.emit("session_shutdown", "quit");
    }
  } finally {
    vi.unstubAllEnvs();
  }
});

test.each(["before", "after"])(
  "duplicate loaded %s binding preserves child identity",
  async (when) => {
    const first = fakePi();
    const duplicate = fakePi();
    (await load())(first.pi);
    if (when === "before") (await load())(duplicate.pi);
    await first.emit("session_start", "startup");
    if (when === "after") (await load())(duplicate.pi);
    await duplicate.emit("session_start", "startup");

    expect(childSession()).toBe("sess_one");
    expect(duplicate.handlers.size).toBe(0);
    expect(connect).toHaveBeenCalledOnce();
    expect(diagnostic).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ code: "duplicate_extension" }),
    );
  },
);

test("reload releases ownership so a different copy can restore the session", async () => {
  const first = fakePi();
  (await load())(first.pi);
  await first.emit("session_start", "startup");
  await first.emit("session_shutdown", "reload");
  expect(childSession()).toBe("unset");

  const replacement = fakePi();
  (await load())(replacement.pi);
  await replacement.emit("session_start", "reload");
  const duplicate = fakePi();
  (await load())(duplicate.pi);
  expect(childSession()).toBe("sess_one");
  expect(duplicate.handlers.size).toBe(0);
  expect(connect).toHaveBeenCalledTimes(2);
});

test.each(["new", "resume", "fork"])(
  "%s keeps ownership while changing the session",
  async (reason) => {
    const first = fakePi();
    (await load())(first.pi);
    await first.emit("session_start", "startup");
    await first.emit("session_shutdown", reason);
    const duplicate = fakePi();
    (await load())(duplicate.pi);
    await first.emit("session_start", reason, "two");
    expect(childSession()).toBe("sess_two");
    expect(duplicate.handlers.size).toBe(0);
  },
);

test("failed initialization releases ownership for the next copy", async () => {
  const broken = fakePi();
  vi.mocked(broken.pi.registerCommand).mockImplementationOnce(() => {
    throw new Error("Registration failed");
  });
  const initialize = await load();
  expect(() => initialize(broken.pi)).toThrow("Registration failed");

  const next = fakePi();
  (await load())(next.pi);
  await next.emit("session_start", "startup");
  expect(childSession()).toBe("sess_one");
});
