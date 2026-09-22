// Cross-language fixture: exercise the actual extension listener from Rust.
import { startControlSocket } from "../src/control-socket.ts";

const [directory, sessionId, runtimeInstanceId] = process.argv.slice(2);
const server = await startControlSocket({ sessionId, runtimeInstanceId }, { XDG_RUNTIME_DIR: directory });
process.stdout.write(`${server.socketPath}\n`);
process.stdin.resume();
process.stdin.once("end", async () => {
  await server.close();
});
