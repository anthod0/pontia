import { bootstrap, serve } from "./server.js";
import { readHistory } from "./history.js";
import { preflight } from "./preflight.js";

async function main(): Promise<void> {
  if (process.argv[2] === "preflight") {
    await preflight();
    return;
  }
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

main().catch((error: unknown) => {
  if (process.argv[2] !== "preflight" && process.argv[2] !== "history") {
    const systemCode = error instanceof Error && "code" in error ? error.code : undefined;
    const code =
      typeof systemCode === "string" &&
      ["EACCES", "EADDRINUSE", "EINVAL", "ENOENT", "ENAMETOOLONG", "ENOTDIR", "EPERM"].includes(
        systemCode,
      )
        ? systemCode
        : "HOST_START_FAILED";
    process.stdout.write(JSON.stringify({ listening: false, code }) + "\n");
  }
  process.stderr.write("Claude Agent host could not start.\n");
  process.exitCode = 1;
});
