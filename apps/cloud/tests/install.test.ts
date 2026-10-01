import { afterAll, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

const testRootPromise = mkdtemp(join(tmpdir(), "pontia-installer-"));

afterAll(async () => {
  const testRoot = await testRootPromise;
  if (!resolve(testRoot).startsWith(resolve(tmpdir()) + sep)) {
    throw new Error("Invalid test root");
  }
  await rm(testRoot, { recursive: true, force: true });
});

test("installer downloads, verifies, and installs pontia and pontiad", async () => {
  const testRoot = await testRootPromise;
  const payloads = join(testRoot, "payloads");
  const mocks = join(testRoot, "bin");
  const releases = join(testRoot, "releases");
  const requests = join(testRoot, "requests");
  const installDirectory = join(testRoot, "install");
  await mkdir(payloads);
  await mkdir(mocks);
  await mkdir(releases);

  const target =
    process.arch === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  const checksums: string[] = [];

  for (const binary of ["pontia", "pontiad"]) {
    const payload = join(payloads, binary);
    const archiveName = `${binary}-${target}.tar.gz`;
    const archive = join(releases, archiveName);
    await mkdir(payload);
    await writeFile(join(payload, binary), `verified ${binary} release binary\n`, { mode: 0o755 });
    const packed = Bun.spawnSync(["tar", "-czf", archive, "-C", payload, binary]);
    expect(packed.exitCode).toBe(0);
    const checksum = createHash("sha256")
      .update(await readFile(archive))
      .digest("hex");
    checksums.push(`${checksum}  ${archiveName}`);
  }
  await writeFile(join(releases, "SHA256SUMS"), `${checksums.join("\n")}\n`);

  const curl = join(mocks, "curl");
  await writeFile(
    curl,
    `#!/bin/sh
set -eu
url="$2"
out="$4"
printf '%s\n' "$url" >> "$TEST_REQUESTS"
cp "$TEST_RELEASES/\${url##*/}" "$out"
`,
  );
  await chmod(curl, 0o755);

  const processResult = Bun.spawnSync(["sh", "static/install.sh"], {
    cwd: new URL("..", import.meta.url).pathname,
    env: {
      ...process.env,
      PATH: `${mocks}:${process.env.PATH}`,
      PONTIA_INSTALL_DIR: installDirectory,
      PONTIA_VERSION: "v1.2.3",
      TEST_RELEASES: releases,
      TEST_REQUESTS: requests,
    },
  });

  expect(processResult.exitCode).toBe(0);
  expect(await readFile(join(installDirectory, "pontia"), "utf8")).toBe(
    "verified pontia release binary\n",
  );
  expect(await readFile(join(installDirectory, "pontiad"), "utf8")).toBe(
    "verified pontiad release binary\n",
  );
  expect(await readFile(requests, "utf8")).toBe(
    `https://github.com/anthod0/pontia/releases/download/v1.2.3/SHA256SUMS\nhttps://github.com/anthod0/pontia/releases/download/v1.2.3/pontia-${target}.tar.gz\nhttps://github.com/anthod0/pontia/releases/download/v1.2.3/pontiad-${target}.tar.gz\n`,
  );
});
