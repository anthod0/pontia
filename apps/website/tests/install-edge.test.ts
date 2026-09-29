import { afterAll, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

const testRootPromise = mkdtemp(join(tmpdir(), "pontia-edge-installer-"));

afterAll(async () => {
  const testRoot = await testRootPromise;
  if (!resolve(testRoot).startsWith(resolve(tmpdir()) + sep)) {
    throw new Error("Invalid test root");
  }
  await rm(testRoot, { recursive: true, force: true });
});

test("installer downloads, verifies, and installs only the release binary", async () => {
  const testRoot = await testRootPromise;
  const payload = join(testRoot, "payload");
  const mocks = join(testRoot, "bin");
  const archive = join(testRoot, "pontia-edge.tar.gz");
  const requests = join(testRoot, "requests");
  const installDirectory = join(testRoot, "install");
  await mkdir(payload);
  await mkdir(mocks);
  await writeFile(join(payload, "pontia-edge"), "verified release binary\n", { mode: 0o755 });
  const packed = Bun.spawnSync(["tar", "-czf", archive, "-C", payload, "pontia-edge"]);
  expect(packed.exitCode).toBe(0);
  const checksum = createHash("sha256")
    .update(await readFile(archive))
    .digest("hex");

  const curl = join(mocks, "curl");
  await writeFile(
    curl,
    `#!/bin/sh
set -eu
url="$2"
out="$4"
printf '%s\\n' "$url" >> "$TEST_REQUESTS"
case "$url" in
  */SHA256SUMS) printf '%s  %s\\n' "$TEST_CHECKSUM" "$TEST_ARCHIVE_NAME" > "$out" ;;
  *) cp "$TEST_ARCHIVE" "$out" ;;
esac
`,
  );
  await chmod(curl, 0o755);

  const target =
    process.arch === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  const archiveName = `pontia-edge-${target}.tar.gz`;
  const processResult = Bun.spawnSync(["sh", "static/install-edge.sh"], {
    cwd: new URL("..", import.meta.url).pathname,
    env: {
      ...process.env,
      PATH: `${mocks}:${process.env.PATH}`,
      PONTIA_EDGE_INSTALL_DIR: installDirectory,
      PONTIA_EDGE_VERSION: "v1.2.3",
      TEST_ARCHIVE: archive,
      TEST_ARCHIVE_NAME: archiveName,
      TEST_CHECKSUM: checksum,
      TEST_REQUESTS: requests,
    },
  });

  expect(processResult.exitCode).toBe(0);
  expect(await readFile(join(installDirectory, "pontia-edge"), "utf8")).toBe(
    "verified release binary\n",
  );
  expect(await readFile(requests, "utf8")).toBe(
    `https://github.com/anthod0/pontia/releases/download/v1.2.3/${archiveName}\nhttps://github.com/anthod0/pontia/releases/download/v1.2.3/SHA256SUMS\n`,
  );
});
