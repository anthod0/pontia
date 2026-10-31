import { access } from "node:fs/promises";
import { constants } from "node:fs";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

export async function preflight(): Promise<void> {
  // Inspect only runtime dependencies. Never start a Query or read authentication.
  const binary = fileURLToPath(
    import.meta.resolve(
      `@anthropic-ai/claude-agent-sdk-${process.platform}-${process.arch}/claude`,
    ),
  );
  await access(binary, constants.X_OK);
  await promisify(execFile)(binary, ["--version"]);
}
