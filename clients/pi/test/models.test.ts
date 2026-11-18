import { realpath } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import type { InternalEvent } from "../src/events.js";
import { createPontiaPiExtension } from "../src/index.js";
import { tempDir } from "./temp-dir.js";

async function fixture(failInitialModelReport = false) {
  const root = await tempDir("pm-");
  const workspace = await realpath(root);
  const handlers: Record<string, (event: any, ctx?: any) => Promise<void>> = {};
  const events: InternalEvent[] = [];
  const controls: Array<() => Promise<void>> = [];
  const close = vi.fn(async () => {});
  let failModelReports = failInitialModelReport;
  const available = [
    { provider: "one", id: "shared/name", name: "First" },
    { provider: "two", id: "shared/name", name: "Second" },
  ];
  let current = available[0];
  const context = {
    mode: "tui",
    get model() {
      return current;
    },
    sessionManager: {
      getSessionId: () => "native",
      getSessionFile: () => join(root, "pi.jsonl"),
      getCwd: () => workspace,
    },
  };
  createPontiaPiExtension(
    {
      on(name: string, handler: any) {
        handlers[name] = handler;
      },
      registerCommand() {},
    } as any,
    {
      env: { PONTIA_HOME: root },

      logDiagnostic: vi.fn(async () => {}),
      connectPi: async (_home, _error, _submit, onReconnect) => {
        controls.push(onReconnect!);
        return {
          close,
          registered() {},
          async request(method, params) {
            if (method === "event.report") {
              const event = (params as any).event;
              if (failModelReports && event.type === "session.model_updated")
                throw new Error("Model report acknowledgement lost");
              events.push(event);
              return { accepted: true };
            }
            if (method === "workspaces.list")
              return { workspaces: [{ canonical_path: workspace, state: "active" }] };
            if (method === "session.context")
              return {
                session_context: {
                  session_id: "sess_models",
                  runtime_id: "rt_models",
                  client_type: "pi",
                  session_state: "starting",
                },
              };
            return {
              session: { session_id: "sess_models" },
              runtime: { runtime_id: "rt_models" },
            };
          },
        };
      },
    },
  );
  await handlers.session_start({ reason: "startup" }, context);
  return {
    events,
    controls,
    available,
    close,
    failReports(value: boolean) {
      failModelReports = value;
    },
    async nativeSelect(model: typeof current) {
      current = model;
      await handlers.model_select({ model, source: "cycle" }, context);
    },
  };
}

test("Pi reports provider-qualified models at startup, on native changes and on reconnect", async () => {
  const f = await fixture();
  const control = f.controls[0];
  expect(f.events.at(-1)).toMatchObject({
    type: "session.model_updated",
    data: { model: "one/shared/name", runtime_id: "rt_models" },
  });
  await f.nativeSelect(f.available[1]);
  expect(f.events.at(-1)?.data.model).toBe("two/shared/name");
  f.events.length = 0;
  await control();
  expect(f.events).toHaveLength(1);
  expect(f.events[0].data.model).toBe("two/shared/name");
});

test("failed startup model reporting preserves the registered connection for resynchronization", async () => {
  const f = await fixture(true);
  expect(f.events.map((event) => event.type)).toEqual(["session.ready"]);
  expect(f.close).not.toHaveBeenCalled();
  f.failReports(false);
  await f.controls[0]();
  expect(f.events.at(-1)?.data.model).toBe("one/shared/name");
});
