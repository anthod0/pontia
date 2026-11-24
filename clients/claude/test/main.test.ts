import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtempDisposable } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

for (const invalidToken of [false, true]) {
  test(
    `startup reports a safe ${invalidToken ? "validation" : "filesystem"} error`,
    { timeout: 5000 },
    async (t) => {
      const sandbox = await mkdtempDisposable(join(tmpdir(), "pontia-startup-test-"));
      t.after(() => sandbox.remove());
      const child = spawn(
        process.execPath,
        [fileURLToPath(new URL("../src/main.js", import.meta.url))],
        {
          stdio: ["pipe", "pipe", "pipe"],
        },
      );
      t.after(() => {
        child.kill();
      });
      let stdout = "";
      let stderr = "";
      child.stdout.on("data", (chunk) => {
        stdout += chunk.toString();
      });
      child.stderr.on("data", (chunk) => {
        stderr += chunk.toString();
      });
      const closed = once(child, "close");
      child.stdin.end(
        JSON.stringify({
          session_id: "private-session",
          runtime_id: "private-runtime",
          launch: { cwd: sandbox.path, mode: "default" },
          pontia_home: invalidToken ? 1 : join(sandbox.path, "private-missing-directory"),
        }) + "\n",
      );
      const [code] = await closed;
      assert.equal(code, 1);
      assert.equal(stdout, "");
      assert.notEqual(stderr, "");
      assert.doesNotMatch(stderr, /private-/);
    },
  );
}
