import { realpath } from "node:fs/promises";
import { resolve } from "node:path";
import type { EnvLike } from "./context.js";
import { resolvePontiaConnection } from "./discovery.js";
import { asRecord, fetchJson, optionalString, responseDataRecord } from "./internal-api.js";

function externalApiUrl(env: EnvLike): string | undefined {
  return optionalString(env.PONTIA_EXTERNAL_API_URL)?.replace(/\/+$/, "");
}

async function canonicalPath(path: string): Promise<string> {
  try {
    return await realpath(path);
  } catch {
    return resolve(path);
  }
}

export async function resolveWorkspaceApi(env: EnvLike, fetchImpl: typeof fetch): Promise<{ externalApiUrl: string; externalApiToken: string } | undefined> {
  const explicitUrl = externalApiUrl(env);
  const explicitToken = optionalString(env.PONTIA_EXTERNAL_API_TOKEN);
  if (explicitUrl && explicitToken) return { externalApiUrl: explicitUrl, externalApiToken: explicitToken };

  const discovered = await resolvePontiaConnection({ env, fetch: fetchImpl });
  if (!discovered?.externalApiToken) return undefined;
  return { externalApiUrl: discovered.externalApiUrl, externalApiToken: discovered.externalApiToken };
}

export async function isActiveRegisteredWorkspace(env: EnvLike, fetchImpl: typeof fetch, clientCwd: string | undefined): Promise<boolean | undefined> {
  if (!clientCwd) return false;
  const api = await resolveWorkspaceApi(env, fetchImpl);
  if (!api) return undefined;

  const workspacePath = await canonicalPath(clientCwd);
  const body = await fetchJson(fetchImpl, `${api.externalApiUrl}/workspaces`, api.externalApiToken);
  const workspaces = responseDataRecord(body)?.workspaces;
  if (!Array.isArray(workspaces)) return false;

  return workspaces.some((workspace) => {
    const record = asRecord(workspace);
    return record?.state === "active" && record.canonical_path === workspacePath;
  });
}
