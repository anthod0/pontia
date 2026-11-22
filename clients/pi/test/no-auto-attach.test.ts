import { describe, expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";

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
  test("does not register Pontia behavior with an invalid PONTIA_HOME", async () => {
    const { pi, handlers } = fakePi();
    const connect = vi.fn(async () => {
      throw new Error("unexpected connection");
    });
    const makeReporter = vi.fn(() => ({ report: vi.fn(async () => ({ accepted: true })) }));

    createPontiaPiExtension(pi as any, {
      env: { PONTIA_HOME: "" },
      connectPi: connect,
      makeReporter,
      logDiagnostic: vi.fn(async () => undefined),
    });

    expect(handlers).toEqual({});
    expect(pi.registerCommand).not.toHaveBeenCalled();
    expect(connect).not.toHaveBeenCalled();
    expect(makeReporter).not.toHaveBeenCalled();
  });
});
