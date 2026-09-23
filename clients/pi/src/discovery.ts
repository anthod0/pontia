import { isAbsolute, join, parse, sep } from "node:path";
import type { EnvLike } from "./context.js";

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

export function pontiaHomeFromEnv(env: EnvLike = process.env): string | undefined {
  if (env.PONTIA_HOME !== undefined) return validRoot(env.PONTIA_HOME);
  const home = validRoot(env.HOME);
  return home ? join(home, ".pontia") : undefined;
}

function validRoot(value: unknown): string | undefined {
  const path = optionalString(value);
  if (!path || !isAbsolute(path) || parse(path).root === path || path.split(sep).includes("..")) return undefined;
  return path;
}
