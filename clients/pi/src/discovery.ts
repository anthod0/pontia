import { readFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import type { EnvLike } from "./context.js";

export interface PontiaConnection {
  baseUrl: string;
  internalEventUrl: string;
  bindingUpsertUrl: string;
  externalApiUrl: string;
  externalApiToken?: string;
}

export interface PontiaDiscoveryOptions {
  pontiaHome?: string;
  env?: EnvLike;
  fetch?: typeof fetch;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

export function pontiaHomeFromEnv(env: EnvLike = process.env): string | undefined {
  const pontiaHome = optionalString(env.PONTIA_HOME);
  return pontiaHome && isAbsolute(pontiaHome) ? pontiaHome : undefined;
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
  return port === "80" ? `http://${localHost}` : `http://${localHost}:${port}`;
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

export async function resolvePontiaConnection(options: PontiaDiscoveryOptions = {}): Promise<PontiaConnection | undefined> {
  const pontiaHome = options.pontiaHome ?? pontiaHomeFromEnv(options.env);
  if (!pontiaHome || !isAbsolute(pontiaHome)) return undefined;
  const configPath = join(pontiaHome, "config.toml");

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
  return connectionFromBaseUrl(baseUrl, parseTomlString(raw, "external_api_token"));
}
