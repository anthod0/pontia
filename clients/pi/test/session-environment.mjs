import { execFileSync } from "node:child_process";
import { registerHooks } from "node:module";
import { join } from "node:path";

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (
      context.parentURL?.startsWith(new URL("../src/", import.meta.url).href) &&
      specifier.endsWith(".js")
    ) {
      specifier = `${specifier.slice(0, -3)}.ts`;
    }
    return nextResolve(specifier, context);
  },
});
const { createPontiaPiExtension } = await import("../src/index.ts");
const [workspace, suffix] = process.argv.slice(2);
const observations = {};
const sessions = new Map();
let managed = true;
let activeWorkspace = true;
let rejectRegistration = false;
let existingState = "exited";

function observe(step) {
  observations[step] = execFileSync("sh", ["-c", 'printf "%s" "${PONTIA_SESSION_ID-unset}"'], {
    encoding: "utf8",
  });
}

function install() {
  const handlers = {};
  createPontiaPiExtension(
    {
      on(name, handler) {
        handlers[name] = handler;
      },
      registerCommand() {},
    },
    {
      isManagedPane: async () => managed,
      logDiagnostic: async () => {},
      makeLiveOutputPublisher: () => ({}),
      connectPi: async () => ({
        registered() {},
        async close() {},
        async request(method, params) {
          if (method === "workspaces.list") {
            return {
              workspaces: [
                { canonical_path: workspace, state: activeWorkspace ? "active" : "archived" },
              ],
            };
          }
          if (method === "session.context") {
            const sessionId = sessions.get(params.client_session_key);
            return {
              session_context: sessionId
                ? {
                    session_id: sessionId,
                    runtime_id: `rt_${sessionId}`,
                    client_type: "pi",
                    session_state: existingState,
                  }
                : null,
            };
          }
          if (method === "runtime.register") {
            if (rejectRegistration) throw new Error("Registration rejected");
            const key = params.binding.client_session_key;
            const sessionId = sessions.get(key) ?? `sess_${suffix}_${sessions.size + 1}`;
            sessions.set(key, sessionId);
            managed = true;
            return {
              session: { session_id: sessionId },
              runtime: { runtime_id: `rt_${sessionId}` },
            };
          }
          if (method === "event.report") return { accepted: true, turn_id: "turn_1" };
          throw new Error(`Unexpected RPC: ${method}`);
        },
      }),
    },
  );
  return handlers;
}

function context(key) {
  return {
    mode: "tui",
    sessionManager: {
      getSessionId: () => key,
      getSessionFile: () => join(workspace, `${key}.jsonl`),
      getCwd: () => workspace,
      getLeafId: () => "entry_1",
    },
  };
}

let handlers = install();
observe("unbound");
process.env.PONTIA_SESSION_ID = "wrong-before-binding";
await handlers.session_start({ reason: "startup" }, context("native_one"));
observe("created");
await handlers.session_shutdown({ reason: "quit" });
observe("shutdown");
handlers = install();
await handlers.session_start({ reason: "startup" }, context("native_one"));
observe("restored");
await handlers.session_shutdown({ reason: "reload" });
observe("reloading");
handlers = install();
existingState = "idle";
await handlers.session_start({ reason: "reload" }, context("native_one"));
observe("reloaded");
existingState = "exited";
await handlers.session_shutdown({ reason: "new" });
observe("switching");
await handlers.session_start({ reason: "new" }, context("native_two"));
observe("switched");
await handlers.session_shutdown({ reason: "fork" });
await handlers.session_start({ reason: "fork" }, context("native_fork"));
observe("forked");
await handlers.session_shutdown({ reason: "resume" });
await handlers.session_start({ reason: "resume" }, context("native_one"));
observe("returned");
rejectRegistration = true;
await handlers.session_start({ reason: "new" }, context("native_failed"));
observe("failed");
rejectRegistration = false;

handlers = install();
managed = false;
await handlers.session_start({ reason: "startup" }, context("native_manual"));
observe("deferred");
await handlers.agent_start({}, context("native_manual"));
observe("manual");
await handlers.session_shutdown({ reason: "quit" });

for (const scenario of ["inactive", "duplicate", "missing_reload", "non_tui"]) {
  process.env.PONTIA_SESSION_ID = "stale";
  handlers = install();
  activeWorkspace = scenario !== "inactive";
  existingState = "idle";
  await handlers.session_start(
    { reason: scenario === "missing_reload" ? "reload" : "startup" },
    {
      ...context(scenario === "missing_reload" ? "unknown" : "native_one"),
      mode: scenario === "non_tui" ? "rpc" : "tui",
    },
  );
  observe(scenario);
}
process.stdout.write(JSON.stringify(observations));
