#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, renameSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { delimiter, dirname, join, resolve } from "node:path";

const BINARIES = ["pontia", "pontiad"];
const PLATFORM_PACKAGES = {
  "linux-x64": "@pontia/cli-linux-x64",
  "linux-arm64": "@pontia/cli-linux-arm64",
  "darwin-x64": "@pontia/cli-darwin-x64",
  "darwin-arm64": "@pontia/cli-darwin-arm64",
};
const USAGE = `Usage: npx @pontia/cli install
       npx @pontia/cli <init|update> [pontia arguments...]`;
const CURL_INSTALL = "curl -fsSL https://get.pontia.dev/install.sh | sh";

class InstallerError extends Error {}

function installDir() {
  const configured = process.env.PONTIA_INSTALL_DIR;
  if (configured) return resolve(configured);
  const home = process.env.HOME;
  if (!home) throw new InstallerError("HOME must be set when PONTIA_INSTALL_DIR is not set");
  return join(home, ".local", "bin");
}

function isInstalled(directory) {
  return BINARIES.every((binary) => existsSync(join(directory, binary)));
}

function platformPackageDir() {
  const platform = `${process.platform}-${process.arch}`;
  const name = PLATFORM_PACKAGES[platform];
  if (!name) throw new InstallerError(`unsupported platform: ${platform}`);
  try {
    return dirname(createRequire(import.meta.url).resolve(`${name}/package.json`));
  } catch (error) {
    if (error.code !== "MODULE_NOT_FOUND") throw error;
    throw new InstallerError(
      `${name} is not installed. Reinstall without --omit=optional, or install with: ${CURL_INSTALL}`,
    );
  }
}

// Stage both binaries next to their destinations, then rename, matching scripts/install.sh.
function install(sourceDir, directory) {
  mkdirSync(directory, { recursive: true, mode: 0o755 });
  const staged = [];
  try {
    for (const binary of BINARIES) {
      const path = join(directory, `.${binary}.${randomBytes(6).toString("hex")}`);
      staged.push(path);
      copyFileSync(join(sourceDir, binary), path);
      chmodSync(path, 0o755);
    }
    BINARIES.forEach((binary, index) => {
      renameSync(staged[index], join(directory, binary));
      staged[index] = undefined;
    });
  } finally {
    for (const path of staged) if (path) rmSync(path, { force: true });
  }
  console.log(`Installed pontia and pontiad to ${directory}`);
}

function onPath(directory) {
  return (process.env.PATH ?? "")
    .split(delimiter)
    .some((entry) => entry && resolve(entry) === directory);
}

function runPontia(directory, args) {
  const result = spawnSync(join(directory, "pontia"), args, { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.signal) process.kill(process.pid, result.signal);
  return result.status ?? 1;
}

// Install the packaged binaries unless both are already present.
function ensureInstalled(directory) {
  if (!isInstalled(directory)) install(platformPackageDir(), directory);
  if (!onPath(directory)) {
    console.error(`${directory} is not in PATH. Add it to PATH to run pontia directly.`);
  }
}

function main([command, ...args]) {
  if (command === "install" && args.length === 0) {
    ensureInstalled(installDir());
    return 0;
  }
  if (command !== "init" && command !== "update") {
    console.error(USAGE);
    return 2;
  }
  const directory = installDir();
  if (command === "init") {
    ensureInstalled(directory);
  } else if (!isInstalled(directory)) {
    throw new InstallerError(`Pontia is not installed in ${directory}. Run: npx @pontia/cli init`);
  }
  return runPontia(directory, [command, ...args]);
}

try {
  process.exitCode = main(process.argv.slice(2));
} catch (error) {
  // Report expected failures and system errors (permissions, missing files) without a stack trace.
  if (!(error instanceof InstallerError) && typeof error.code !== "string") throw error;
  console.error(`pontia-install: ${error.message}`);
  process.exitCode = 1;
}
