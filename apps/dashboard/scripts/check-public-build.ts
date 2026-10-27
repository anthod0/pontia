import { readFileSync, readdirSync } from "node:fs";
import { relative, resolve } from "node:path";

// Verifies that a Public Dashboard build keeps remote access out of the SPA bundle.
const dist = resolve(import.meta.dirname, "../dist");
const remoteAccess = resolve(dist, "_remote-access");
const entry = resolve(remoteAccess, "remote-access.js");
// Strings that only remote access code contains: wasm bindings, Cloud API paths and E2E framing.
const markers = ["__wbindgen", "/api/dashboard/devices", "application/pontia-e2e"];

function files(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((item) => {
    const file = resolve(directory, item.name);
    return item.isDirectory() ? files(file) : [file];
  });
}

const all = files(dist);
const violations: string[] = [];
if (!all.includes(entry)) violations.push(`${relative(dist, entry)} is missing.`);
for (const file of all) {
  if (file.startsWith(`${remoteAccess}/`)) continue;
  if (file.endsWith(".wasm"))
    violations.push(`${relative(dist, file)}: wasm outside remote access.`);
  if (!/\.(js|html)$/.test(file)) continue;
  const source = readFileSync(file, "utf8");
  for (const marker of markers.filter((marker) => source.includes(marker))) {
    violations.push(`${relative(dist, file)}: contains remote access code (${marker}).`);
  }
}

if (violations.length > 0) {
  console.error(violations.join("\n"));
  process.exitCode = 1;
} else {
  console.log("Public build keeps remote access separate from the SPA.");
}
