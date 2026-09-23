// Cross-language fixture: the production Pi client connects to the Rust listener.
import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { connectPi, CONTROL_VERSION } from "../src/control-socket.ts";

const [directory, sessionId, runtimeInstanceId, clientSessionKey] = process.argv.slice(2);
process.stdin.resume();
const identity = { sessionId, runtimeInstanceId, clientSessionKey };
const client = await connectPi(directory, (error) => process.stderr.write(`${error}\n`),
  (input) => appendFileSync(join(directory, "messages.jsonl"), `${JSON.stringify(input)}\n`));
await client.request("runtime.attach", { version: CONTROL_VERSION, session_id: sessionId, runtime_instance_id: runtimeInstanceId, client_session_key: clientSessionKey });
client.registered(identity);
process.stdout.write("connected\n");
process.stdin.once("end", async () => { await client.close(); });
