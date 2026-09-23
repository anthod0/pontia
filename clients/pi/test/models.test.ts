import { realpath, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import type { ModelControl } from "../src/control-socket.js";
import type { InternalEvent } from "../src/events.js";
import { createPontiaPiExtension } from "../src/index.js";
import { tempDir } from "./temp-dir.js";

async function fixture(failInitialModelReport = false) {
  const root = await tempDir("pm-");
  const workspace = await realpath(root);
  await writeFile(join(root, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');
  const handlers: Record<string, (event: any, ctx?: any) => Promise<void>> = {};
  const events: InternalEvent[] = [];
  const controls: ModelControl[] = [];
  const close = vi.fn(async () => {});
  let failModelReports = failInitialModelReport;
  const available = [
    { provider: "one", id: "shared/name", name: "First" },
    { provider: "two", id: "shared/name", name: "Second" },
  ];
  let current = available[0];
  const context = {
    mode: "tui",
    get model() { return current; },
    modelRegistry: { getAvailable: () => available },
    sessionManager: { getSessionId: () => "native", getSessionFile: () => join(root, "pi.jsonl"), getCwd: () => workspace },
  };
  const setModel = vi.fn(async (model: typeof current) => {
    const previousModel = current;
    current = model;
    if (previousModel !== model) await handlers.model_select({ model, previousModel, source: "set" }, context);
    return true;
  });
  createPontiaPiExtension({ on(name: string, handler: any) { handlers[name] = handler; }, registerCommand() {}, setModel } as any, {
    env: { PONTIA_HOME: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
    fetch: vi.fn(async () => Response.json({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } })) as typeof fetch,
    isManagedPane: async () => true,
    logDiagnostic: vi.fn(async () => {}),
    connectPi: async (_home, _error, _submit, models) => {
      controls.push(models!);
      return { close, registered() {}, async request(method, params) {
        if (method === "event.report") {
          const event = (params as any).event;
          if (failModelReports && event.type === "session.model_updated") throw new Error("Model report acknowledgement lost");
          events.push(event);
          return { accepted: true };
        }
        if (method === "session.context") return { session_context: null };
        return { session: { session_id: "sess_models" }, runtime: { runtime_instance_id: "rt_models", internal_event_url: "unused" } };
      } };
    },
  });
  await handlers.session_start({ reason: "startup" }, context);
  return { handlers, events, controls, available, context, setModel, close, failReports(value: boolean) { failModelReports = value; }, get current() { return current; },
    async nativeSelect(model: typeof current) { current = model; await handlers.model_select({ model, source: "cycle" }, context); },
  };
}

test("Pi model catalog distinguishes providers and reports startup, native, external and reconnect observations", async () => {
  const f = await fixture();
  const control = f.controls[0];
  expect(control.listModels().map((model) => model.id)).toEqual(["one/shared/name", "two/shared/name"]);
  expect(f.events.at(-1)).toMatchObject({ type: "session.model_updated", data: { model: "one/shared/name", runtime_instance_id: "rt_models" } });
  await control.setModel("two/shared/name");
  expect(f.current).toBe(f.available[1]);
  expect(f.events.at(-1)?.data.model).toBe("two/shared/name");
  await f.nativeSelect(f.available[0]);
  expect(f.events.at(-1)?.data.model).toBe("one/shared/name");
  f.events.length = 0;
  await control.onReconnect();
  expect(f.events).toHaveLength(1);
  expect(f.events[0].data.model).toBe("one/shared/name");
  f.events.length = 0;
  await control.setModel("one/shared/name");
  expect(f.events.at(-1)?.data.model).toBe("one/shared/name");
});

test("unavailable models, auth failure and old control callbacks cannot change the current model", async () => {
  const f = await fixture();
  const control = f.controls[0];
  await expect(control.setModel("missing/model")).rejects.toThrow();
  expect(f.setModel).not.toHaveBeenCalled();
  f.setModel.mockResolvedValueOnce(false);
  const count = f.events.length;
  await expect(control.setModel("two/shared/name")).rejects.toThrow();
  expect(f.events).toHaveLength(count);
  expect(f.current).toBe(f.available[0]);
  await f.handlers.session_shutdown({ reason: "new" });
  await f.handlers.session_start({ reason: "new" }, f.context);
  f.setModel.mockClear();
  await expect(control.setModel("two/shared/name")).rejects.toThrow();
  expect(() => control.listModels()).toThrow();
  await expect(control.onReconnect()).rejects.toThrow();
  expect(f.setModel).not.toHaveBeenCalled();
});

test("concurrent model changes are rejected while the native API is pending", async () => {
  const f = await fixture();
  let release!: (accepted: boolean) => void;
  f.setModel.mockImplementationOnce(() => new Promise<boolean>((resolve) => { release = resolve; }));
  const first = f.controls[0].setModel("two/shared/name");
  await expect(f.controls[0].setModel("one/shared/name")).rejects.toThrow();
  release(false);
  await expect(first).rejects.toThrow();
  expect(f.setModel).toHaveBeenCalledOnce();
});


test("failed startup model reporting preserves the registered connection for resynchronization", async () => {
  const f = await fixture(true);
  expect(f.events.map((event) => event.type)).toEqual(["session.ready"]);
  expect(f.close).not.toHaveBeenCalled();
  expect(f.controls[0].listModels()).toHaveLength(2);
  f.failReports(false);
  await f.controls[0].onReconnect();
  expect(f.events.at(-1)?.data.model).toBe("one/shared/name");
});

test("a native model change with a lost observation acknowledgement is unknown, not rejected", async () => {
  const f = await fixture();
  f.failReports(true);
  await expect(f.controls[0].setModel("two/shared/name")).rejects.toMatchObject({ code: -32007 });
  expect(f.current).toBe(f.available[1]);
  f.failReports(false);
  await f.controls[0].onReconnect();
  expect(f.events.at(-1)?.data.model).toBe("two/shared/name");
});
