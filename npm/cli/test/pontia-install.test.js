import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { fileURLToPath } from "node:url";

const entry = fileURLToPath(new URL("../bin/pontia-install.js", import.meta.url));
const platformPackage = `cli-${process.platform}-${process.arch}`;

let testRoot;
let installDir;
let cliBin;

// Fake pontia records its arguments and exits with PONTIA_FAKE_EXIT.
function fakeBinary(path, label) {
  writeFileSync(
    path,
    `#!/bin/sh\nprintf '%s %s\\n' '${label}' "$*" > "$PONTIA_FAKE_LOG"\nexit "\${PONTIA_FAKE_EXIT:-0}"\n`,
    { mode: 0o755 },
  );
}

function createPlatformPackage() {
  const dir = join(testRoot, "node_modules", "@pontia", platformPackage);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "package.json"), JSON.stringify({ name: `@pontia/${platformPackage}` }));
  fakeBinary(join(dir, "pontia"), "packaged");
  fakeBinary(join(dir, "pontiad"), "packaged-daemon");
}

function run(args, env = {}) {
  return spawnSync(process.execPath, [cliBin, ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      PONTIA_INSTALL_DIR: installDir,
      PONTIA_FAKE_LOG: join(testRoot, "pontia.log"),
      ...env,
    },
  });
}

function invocation() {
  return readFileSync(join(testRoot, "pontia.log"), "utf8").trim();
}

beforeEach(() => {
  testRoot = mkdtempSync(join(tmpdir(), "pontia-npm-cli-test-"));
  installDir = join(testRoot, "install", "bin");
  const cliDir = join(testRoot, "node_modules", "@pontia", "cli", "bin");
  mkdirSync(cliDir, { recursive: true });
  writeFileSync(
    join(testRoot, "node_modules", "@pontia", "cli", "package.json"),
    JSON.stringify({ type: "module" }),
  );
  cliBin = join(cliDir, "pontia-install.js");
  copyFileSync(entry, cliBin);
});

afterEach(() => {
  rmSync(testRoot, { recursive: true, force: true });
});

test("init installs packaged binaries and runs pontia init with forwarded arguments", () => {
  createPlatformPackage();

  const result = run(["init", "--yes"], { PONTIA_FAKE_EXIT: "7" });

  assert.equal(result.status, 7);
  assert.equal(invocation(), "packaged init --yes");
  for (const binary of ["pontia", "pontiad"]) {
    assert.equal(statSync(join(installDir, binary)).mode & 0o777, 0o755);
  }
});

test("init keeps an existing installation instead of overwriting it", () => {
  createPlatformPackage();
  mkdirSync(installDir, { recursive: true });
  fakeBinary(join(installDir, "pontia"), "existing");
  fakeBinary(join(installDir, "pontiad"), "existing-daemon");

  const result = run(["init"]);

  assert.equal(result.status, 0);
  assert.equal(invocation(), "existing init");
});

test("init reinstalls both binaries when only pontia is present", () => {
  createPlatformPackage();
  mkdirSync(installDir, { recursive: true });
  fakeBinary(join(installDir, "pontia"), "existing");

  const result = run(["init"]);

  assert.equal(result.status, 0);
  assert.equal(invocation(), "packaged init");
  assert.match(readFileSync(join(installDir, "pontiad"), "utf8"), /packaged-daemon/);
  assert.deepEqual(readdirSync(installDir).sort(), ["pontia", "pontiad"]);
});

test("init fails without installing when the platform package is missing", () => {
  const result = run(["init"]);

  assert.equal(result.status, 1);
  assert.equal(existsSync(join(installDir, "pontia")), false);
});

test("update runs the installed pontia update with forwarded arguments", () => {
  mkdirSync(installDir, { recursive: true });
  fakeBinary(join(installDir, "pontia"), "existing");
  fakeBinary(join(installDir, "pontiad"), "existing-daemon");

  const result = run(["update", "--check"], { PONTIA_FAKE_EXIT: "3" });

  assert.equal(result.status, 3);
  assert.equal(invocation(), "existing update --check");
});

test("update fails without installing when Pontia is not installed", () => {
  createPlatformPackage();

  const result = run(["update"]);

  assert.equal(result.status, 1);
  assert.equal(existsSync(join(installDir, "pontia")), false);
});

test("install installs packaged binaries without running pontia", () => {
  createPlatformPackage();

  const result = run(["install"]);

  assert.equal(result.status, 0);
  assert.equal(existsSync(join(testRoot, "pontia.log")), false);
  assert.match(readFileSync(join(installDir, "pontia"), "utf8"), /packaged/);
  for (const binary of ["pontia", "pontiad"]) {
    assert.equal(statSync(join(installDir, binary)).mode & 0o777, 0o755);
  }
});

test("install keeps an existing installation instead of overwriting it", () => {
  createPlatformPackage();
  mkdirSync(installDir, { recursive: true });
  fakeBinary(join(installDir, "pontia"), "existing");
  fakeBinary(join(installDir, "pontiad"), "existing-daemon");

  const result = run(["install"]);

  assert.equal(result.status, 0);
  assert.match(readFileSync(join(installDir, "pontia"), "utf8"), /existing/);
});

test("install fails when the platform package is missing", () => {
  const result = run(["install"]);

  assert.equal(result.status, 1);
  assert.equal(existsSync(join(installDir, "pontia")), false);
});

test("rejects missing or unknown subcommands", () => {
  createPlatformPackage();

  assert.notEqual(run([]).status, 0);
  assert.notEqual(run(["setup"]).status, 0);
  assert.notEqual(run(["install", "--yes"]).status, 0);
  assert.equal(existsSync(join(installDir, "pontia")), false);
});
