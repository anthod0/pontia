import { bootstrap, serve } from "./server.js";
import { readHistory } from "./history.js";

async function main(): Promise<void> {
  let input = Buffer.alloc(0);
  // The parent supplies the connection capability privately, never in argv or a log.
  for await (const chunk of process.stdin) {
    input = Buffer.concat([input, Buffer.from(chunk)]);
    if (input.length > 64 * 1024) throw new Error("bootstrap_too_large");
    const newline = input.indexOf(10);
    if (newline < 0) continue;
    const data = JSON.parse(input.subarray(0, newline).toString("utf8"));
    if (process.argv[2] === "history") {
      if (typeof data.session_id !== "string" || typeof data.cwd !== "string")
        throw new Error("invalid_history_request");
      process.stdout.write(JSON.stringify(await readHistory(data.session_id, data.cwd)) + "\n");
      return;
    }
    const config = bootstrap(data);
    await serve(config);
    process.stdout.write('{"listening":true}\n');
    return;
  }
  throw new Error("bootstrap_missing");
}

main().catch(() => {
  process.stderr.write("Claude Agent host could not start.\n");
  process.exitCode = 1;
});
