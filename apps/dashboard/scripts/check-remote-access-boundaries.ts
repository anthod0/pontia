import { readFileSync, readdirSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { preprocess } from "svelte/compiler";
import ts from "typescript";

const src = resolve(import.meta.dirname, "../src");
const publicMode = resolve(src, "modes/public");
const remoteAccess = resolve(publicMode, "remote-access");
const wasm = resolve(src, "e2e-wasm");
const handleValidator = resolve(src, "lib/deviceHandle");
// The only SPA module that may reference remote access; it loads the separately built module.
const loader = resolve(publicMode, "remoteAccessLoader.svelte.ts");
const imports: { file: string; specifier: string; target: string }[] = [];

function within(file: string, directory: string): boolean {
  return file === directory || file.startsWith(`${directory}${sep}`);
}

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = resolve(directory, entry.name);
    return entry.isDirectory() ? sourceFiles(file) : /\.(ts|js|svelte)$/.test(file) ? [file] : [];
  });
}

function collectImports(file: string, source: string): void {
  for (const { fileName: specifier } of ts.preProcessFile(source, true, true).importedFiles) {
    const path = specifier.split("?")[0].replace(/\.(ts|js)$/, "");
    const target = path.startsWith(".")
      ? resolve(dirname(file), path)
      : path.startsWith("$lib/")
        ? resolve(src, "lib", path.slice("$lib/".length))
        : path.startsWith("$dashboard-mode/")
          ? resolve(publicMode, path.slice("$dashboard-mode/".length))
          : path;
    imports.push({ file, specifier, target });
  }
}

for (const file of sourceFiles(src)) {
  const source = readFileSync(file, "utf8");
  if (file.endsWith(".svelte")) {
    await preprocess(source, {
      script: ({ content }) => {
        collectImports(file, content);
      },
    });
  } else {
    collectImports(file, source);
  }
}

const violations: string[] = [];
for (const { file, specifier, target } of imports) {
  const report = (message: string) => {
    violations.push(`${relative(src, file)}: ${specifier}: ${message}`);
  };
  if (file === `${handleValidator}.ts`) {
    report("Device handle validation must remain dependency-free.");
  }
  if (
    within(file, remoteAccess) &&
    !within(target, remoteAccess) &&
    target !== handleValidator &&
    !within(target, wasm)
  ) {
    report("Remote access may depend only on its own modules, handle validation and E2E wasm.");
  }
  if (!within(file, remoteAccess) && !within(file, wasm)) {
    if (within(target, wasm)) {
      report("The SPA must not load E2E wasm; it belongs to remote access.");
    } else if (within(target, remoteAccess) && file !== loader) {
      report("The SPA must load remote access through remoteAccessLoader.");
    } else if (
      within(target, remoteAccess) &&
      target !== remoteAccess &&
      target !== resolve(remoteAccess, "index")
    ) {
      report("remoteAccessLoader must use the remote-access entry point.");
    }
  }
  if (
    !within(file, resolve(src, "modes")) &&
    within(target, resolve(src, "modes")) &&
    !specifier.startsWith("$dashboard-mode/")
  ) {
    report("Shared code must reference mode dependencies through $dashboard-mode.");
  }
}

if (violations.length > 0) {
  console.error(violations.join("\n"));
  process.exitCode = 1;
} else {
  console.log("Remote access dependency boundaries passed.");
}
