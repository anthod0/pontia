// Cross-language fixture: exercise the actual extension listener from Rust.
import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { startControlSocket } from "../src/control-socket.ts";

const [directory, sessionId, runtimeInstanceId] = process.argv.slice(2);
const server = await startControlSocket({ sessionId, runtimeInstanceId }, { XDG_RUNTIME_DIR: directory }, undefined,
  (input) => appendFileSync(join(directory, "messages.jsonl"), `${JSON.stringify(input)}\n`));
process.stdout.write(`${server.socketPath}\n`);
process.stdin.resume();
process.stdin.once("end", async () => {
  await server.close();
});
