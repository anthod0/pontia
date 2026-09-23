import { join } from "node:path";
import { createAgentSession, DefaultResourceLoader, ModelRuntime, SessionManager, SettingsManager } from "@earendil-works/pi-coding-agent";
import { expect, onTestFinished, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import { tempDir } from "./temp-dir.js";

test("RPC replay enters Pi's native command context and navigates the running session", async () => {
  const root = await tempDir("pontia-pi-branch-");
  const settingsManager = SettingsManager.inMemory();
  const sessionManager = SessionManager.create(root, join(root, "sessions"));
  const target = sessionManager.appendMessage({ role: "user", content: "original", timestamp: Date.now() });
  sessionManager.appendMessage({ role: "user", content: "later", timestamp: Date.now() });
  const diagnostics = vi.fn();
  const replacement = vi.fn();
  const navigated = vi.fn();
  let replay: ((message: string) => void) | undefined;
  const resourceLoader = new DefaultResourceLoader({
    cwd: root, agentDir: root, settingsManager,
    noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
    extensionFactories: [(pi) => {
      createPontiaPiExtension(pi, {
        env: { PONTIA_HOME: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
        isManagedPane: async () => true,
        logDiagnostic: async (_path, entry) => { diagnostics(entry); },
        makeReporter: () => ({ report: async () => ({ accepted: true }) }),
        connectPi: async (_home, _error, _submit, _models, onReplay) => {
          replay = onReplay;
          return {
            registered() {}, async close() {},
            async request(method, params) {
              if (method === "workspaces.list") return { workspaces: [{ canonical_path: root, state: "active" }] };
              if (method === "session.context") return { session_context: null };
              if (method === "runtime.register") return {
                session: { session_id: "sess_native" },
                runtime: { runtime_instance_id: "rt_native" },
              };
              expect(method).toBe("branch.resolve");
              expect(params).toEqual({
                inbox_message_id: "msg_native", session_id: "sess_native", runtime_instance_id: "rt_native", client_type: "pi",
              });
              return { branch_replay: { ...params, target_entry_id: target, replacement_input: "replacement" } };
            },
          };
        },
      });
      // Observe the native navigation and intercept the replacement before any model call.
      pi.on("session_tree", navigated);
      pi.on("input", (event) => { replacement(event.text); return { action: "handled" }; });
    }],
  });
  await resourceLoader.reload();
  const modelRuntime = await ModelRuntime.create({
    authPath: join(root, "auth.json"), modelsPath: join(root, "models.json"),
    modelsStorePath: join(root, "models-store.json"), refreshOnCreate: false, allowModelNetwork: false,
  });
  const { session } = await createAgentSession({
    cwd: root, agentDir: root, sessionManager, settingsManager, resourceLoader, modelRuntime, tools: [],
  });
  onTestFinished(() => session.dispose());
  const unavailable = async () => { throw new Error("Unexpected session operation"); };
  await session.bindExtensions({
    mode: "tui",
    onError: diagnostics,
    commandContextActions: {
      waitForIdle: () => session.waitForIdle(),
      navigateTree: (entry, options) => session.navigateTree(entry, options),
      newSession: unavailable, fork: unavailable, switchSession: unavailable, reload: unavailable,
    },
  });
  expect(replay).toBeTypeOf("function");
  replay!("msg_native");
  await vi.waitFor(() => expect(replacement).toHaveBeenCalledExactlyOnceWith("replacement"));
  expect(navigated).toHaveBeenCalledOnce();
  expect(sessionManager.getLeafId()).toBeNull();
  expect(diagnostics).not.toHaveBeenCalled();
});
