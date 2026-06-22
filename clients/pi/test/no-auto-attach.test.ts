import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import type { LoadTurnContextResult } from "../src/context.js";

function fakePi() {
  const handlers: Record<string, (event: any, ctx: any) => Promise<any> | any> = {};
  return {
    handlers,
    pi: {
      on: vi.fn((event: string, handler: (event: any, ctx: any) => Promise<any> | any) => {
        handlers[event] = handler;
      }),
      registerTool: vi.fn(),
    },
  };
}

const tmpDirs: string[] = [];

afterEach(async () => {
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
});

async function tempHome() {
  const dir = await mkdtemp(join(tmpdir(), "pontia-pi-no-auto-"));
  tmpDirs.push(dir);
  return dir;
}

describe("pontia pi extension startup boundary", () => {
  test("does not auto-attach or create runtime context after pi has started", async () => {
    const { pi, handlers } = fakePi();
    const fetchImpl = vi.fn(async () => new Response("unexpected", { status: 500 }));
    const makeReporter = vi.fn(() => ({ report: vi.fn(async () => true) }));
    const loadContext = vi.fn(async (): Promise<LoadTurnContextResult> => ({
      ok: false,
      reason: "current turn claim unavailable",
      logFile: "pi-hook.log",
      silent: true,
    }));

    const home = await tempHome();

    createPontiaPiExtension(pi as any, {
      env: { HOME: home },
      fetch: fetchImpl as any,
      loadContext,
      makeReporter,
      logDiagnostic: vi.fn(async () => undefined),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_manual",
        getCwd: () => "/workspace",
      },
    });
    await handlers.before_agent_start({ prompt: "typed in tui", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(makeReporter).not.toHaveBeenCalled();
  });
});
