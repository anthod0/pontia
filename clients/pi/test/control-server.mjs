// Cross-language fixture: the production Pi client connects to the Rust listener.
import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { registerHooks } from "node:module";

// Run the production TypeScript graph directly with Node's type stripping.
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
const { connectPi, CONTROL_VERSION } = await import("../src/control-socket.ts");

const [directory, sessionId, runtimeId, clientSessionKey] = process.argv.slice(2);
process.stdin.resume();
const identity = { sessionId, runtimeId, clientSessionKey };
const client = await connectPi(
  directory,
  (error) => process.stderr.write(`${error}\n`),
  (input) => appendFileSync(join(directory, "messages.jsonl"), `${JSON.stringify(input)}\n`),
  undefined,
  undefined,
  undefined,
  () => ({
    generation: 1,
    sessionManager: {
      getSessionId: () => clientSessionKey,
      getLeafId: () => "answer",
      getEntries: () => [
        {
          id: "user",
          parentId: null,
          type: "message",
          timestamp: "2026-10-11T00:00:00Z",
          message: { role: "user", content: "question" },
        },
        {
          id: "answer",
          parentId: "user",
          type: "message",
          timestamp: "2026-10-11T00:00:00Z",
          message: { role: "assistant", content: [{ type: "text", text: "answer" }] },
        },
      ],
    },
  }),
);
await client.request("runtime.attach", {
  version: CONTROL_VERSION,
  session_id: sessionId,
  runtime_id: runtimeId,
  client_session_key: clientSessionKey,
});
client.registered(identity);
process.stdout.write("connected\n");
process.stdin.once("end", async () => {
  await client.close();
});
