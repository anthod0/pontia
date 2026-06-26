import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import type { EnvLike } from "./context.js";

export interface PontiaConnection {
  baseUrl: string;
  internalEventUrl: string;
  bindingUpsertUrl: string;
  externalApiUrl: string;
  externalApiToken?: string;
}

export interface PontiaDiscoveryOptions {
  env?: EnvLike;
  fetch?: typeof fetch;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function homeDir(env: EnvLike): string {
  return optionalString(env.HOME) ?? homedir();
}

function expandPath(path: string, env: EnvLike): string {
  const home = homeDir(env);
  let expanded = path;
  if (expanded === "~") expanded = home;
  else if (expanded.startsWith("~/")) expanded = join(home, expanded.slice(2));
  expanded = expanded.replace(/^\$HOME(?=\/|$)/, home);
  return isAbsolute(expanded) ? expanded : resolve(expanded);
}

function pontiaHomeDir(env: EnvLike): string {
  return optionalString(env.PONTIA_HOME) ?? join(homeDir(env), ".pontia");
}

function defaultPontiaConfigPath(env: EnvLike): string {
  return join(pontiaHomeDir(env), "config.toml");
}

function parseTomlString(raw: string, key: string): string | undefined {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = raw.match(new RegExp(`^\\s*${escaped}\\s*=\\s*"([^"]*)"`, "m"));
  return optionalString(match?.[1]);
}

function baseUrlFromBindAddr(bindAddr: string): string | undefined {
  const value = bindAddr.trim();
  const bracketMatch = value.match(/^\[([^\]]+)\]:(\d+)$/);
  const plainMatch = value.match(/^([^:]+):(\d+)$/);
  const host = bracketMatch?.[1] ?? plainMatch?.[1];
  const port = bracketMatch?.[2] ?? plainMatch?.[2];
  if (!host || !port) return undefined;
  const localHost = host === "0.0.0.0" || host === "::" || host === "[::]" ? "127.0.0.1" : host;
  return `http://${localHost}:${port}`;
}

function connectionFromBaseUrl(baseUrl: string, externalApiToken?: string): PontiaConnection {
  const normalized = baseUrl.replace(/\/+$/, "");
  return {
    baseUrl: normalized,
    internalEventUrl: `${normalized}/internal/v1/events`,
    bindingUpsertUrl: `${normalized}/internal/v1/runtime-bindings/upsert`,
    externalApiUrl: `${normalized}/external/v1`,
    ...(externalApiToken ? { externalApiToken } : {}),
  };
}

async function isHealthy(fetchImpl: typeof fetch, baseUrl: string): Promise<boolean> {
  try {
    const response = await fetchImpl(`${baseUrl.replace(/\/+$/, "")}/healthz`);
    return response.ok;
  } catch {
    return false;
  }
}

export async function resolvePontiaConnection(options: PontiaDiscoveryOptions = {}): Promise<PontiaConnection | undefined> {
  const env = options.env ?? process.env;
  const fetchImpl = options.fetch ?? fetch;

  const explicitEventUrl = optionalString(env.PONTIA_INTERNAL_EVENT_URL);
  if (explicitEventUrl) {
    const baseUrl = explicitEventUrl.replace(/\/internal\/v1\/events\/?$/, "");
    return connectionFromBaseUrl(baseUrl, optionalString(env.PONTIA_EXTERNAL_API_TOKEN));
  }

  const configFromEnv = optionalString(env.PONTIA_CONFIG);
  const configPath = expandPath(configFromEnv ?? defaultPontiaConfigPath(env), env);

  let raw: string;
  try {
    raw = await readFile(configPath, "utf8");
  } catch {
    return undefined;
  }

  const bindAddr = parseTomlString(raw, "bind_addr");
  if (!bindAddr) return undefined;
  const baseUrl = baseUrlFromBindAddr(bindAddr);
  if (!baseUrl) return undefined;
  if (!(await isHealthy(fetchImpl, baseUrl))) return undefined;

  return connectionFromBaseUrl(baseUrl, optionalString(env.PONTIA_EXTERNAL_API_TOKEN) ?? parseTomlString(raw, "external_api_token"));
}
