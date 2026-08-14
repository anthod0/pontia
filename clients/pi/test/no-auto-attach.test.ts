import { describe, expect, test, vi } from "vitest";
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
      registerCommand: vi.fn(),
    },
  };
}

describe("pontia pi extension startup boundary", () => {
  test("does not register Pontia behavior without PONTIA_HOME", async () => {
    const { pi, handlers } = fakePi();
    const fetchImpl = vi.fn(async () => new Response("unexpected", { status: 500 }));
    const makeReporter = vi.fn(() => ({ report: vi.fn(async () => true) }));
    const loadContext = vi.fn(async (): Promise<LoadTurnContextResult> => ({
      ok: false,
      reason: "current turn claim unavailable",
      logFile: "pi-hook.log",
      silent: true,
    }));

    createPontiaPiExtension(pi as any, {
      env: { TMUX: "/tmp/tmux-1000/default,2071,502", TMUX_PANE: "%42" },
      fetch: fetchImpl as any,
      loadContext,
      makeReporter,
      logDiagnostic: vi.fn(async () => undefined),
    });

    expect(handlers).toEqual({});
    expect(pi.registerCommand).not.toHaveBeenCalled();
    expect(fetchImpl).not.toHaveBeenCalled();
    expect(makeReporter).not.toHaveBeenCalled();
  });
});
