import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import type { EnvLike } from "./context.js";
import { optionalString } from "./internal-api.js";

export interface PontiaConnection {
  internalEventUrl: string;
  externalApiUrl: string;
  externalApiToken?: string;
  bindingUpsertUrl: string;
}

export interface ResolvePontiaConnectionOptions {
  env?: EnvLike;
  fetch?: typeof fetch;
}

function defaultPontiaHome(env: EnvLike): string {
  return env.PONTIA_HOME ?? join(env.HOME ?? homedir(), ".pontia");
}

function normalizeBaseUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, "");
  if (/^https?:\/\//.test(trimmed)) return trimmed;
  const bracketMatch = trimmed.match(/^\[([^\]]+)\]:(\d+)$/);
  const plainMatch = trimmed.match(/^([^:]+):(\d+)$/);
  const host = bracketMatch?.[1] ?? plainMatch?.[1];
  const port = bracketMatch?.[2] ?? plainMatch?.[2];
  if (host && port) {
    const localHost = host === "0.0.0.0" || host === "::" || host === "[::]" ? "127.0.0.1" : host;
    return port === "80" ? `http://${localHost}` : `http://${localHost}:${port}`;
  }
  return `http://${trimmed}`;
}

function parseConfigValue(config: string, key: string): string | undefined {
  const match = config.match(new RegExp(`^\\s*${key}\\s*=\\s*\"([^\"]+)\"`, "m"));
  return match?.[1];
}

export async function resolvePontiaConnection(options: ResolvePontiaConnectionOptions = {}): Promise<PontiaConnection | undefined> {
  const env = options.env ?? process.env;
  const explicitInternal = optionalString(env.PONTIA_INTERNAL_EVENT_URL);
  const explicitExternal = optionalString(env.PONTIA_EXTERNAL_API_URL);
  const explicitToken = optionalString(env.PONTIA_EXTERNAL_API_TOKEN);
  if (explicitInternal || explicitExternal) {
    const externalApiUrl = normalizeBaseUrl(explicitExternal ?? explicitInternal!.replace(/\/internal\/v1\/events\/?$/, "/external/v1"));
    const internalEventUrl = explicitInternal ?? `${externalApiUrl.replace(/\/external\/v1\/?$/, "")}/internal/v1/events`;
    const internalBase = internalEventUrl.replace(/\/events\/?$/, "");
    return {
      internalEventUrl,
      externalApiUrl,
      externalApiToken: explicitToken,
      bindingUpsertUrl: `${internalBase}/runtime-bindings/upsert`,
    };
  }

  try {
    const config = await readFile(join(defaultPontiaHome(env), "config.toml"), "utf8");
    const bindAddr = parseConfigValue(config, "bind_addr");
    if (!bindAddr) return undefined;
    const base = normalizeBaseUrl(bindAddr);
    return {
      internalEventUrl: `${base}/internal/v1/events`,
      externalApiUrl: `${base}/external/v1`,
      externalApiToken: explicitToken ?? parseConfigValue(config, "external_api_token"),
      bindingUpsertUrl: `${base}/internal/v1/runtime-bindings/upsert`,
    };
  } catch {
    return undefined;
  }
}
