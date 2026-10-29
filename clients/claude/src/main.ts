import { bootstrap, serve } from "./server.js";

async function main(): Promise<void> {
  let input = Buffer.alloc(0);
  // The parent supplies the connection capability privately, never in argv or a log.
  for await (const chunk of process.stdin) {
    input = Buffer.concat([input, Buffer.from(chunk)]);
    if (input.length > 64 * 1024) throw new Error("bootstrap_too_large");
    const newline = input.indexOf(10);
    if (newline < 0) continue;
    const config = bootstrap(JSON.parse(input.subarray(0, newline).toString("utf8")));
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
