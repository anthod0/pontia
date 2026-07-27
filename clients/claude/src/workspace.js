import { realpath } from "node:fs/promises";
import { resolve } from "node:path";
import { resolvePontiaConnection } from "./discovery.js";
import { asRecord, fetchJson, responseDataRecord } from "./internal-api.js";
async function canonicalPath(path) {
    try {
        return await realpath(path);
    }
    catch {
        return resolve(path);
    }
}
export async function resolveWorkspaceApi(env, fetchImpl) {
    const discovered = await resolvePontiaConnection({ env, fetch: fetchImpl });
    if (!discovered?.externalApiToken)
        return undefined;
    return { externalApiUrl: discovered.externalApiUrl, externalApiToken: discovered.externalApiToken };
}
export async function isActiveRegisteredWorkspace(env, fetchImpl, clientCwd) {
    if (!clientCwd)
        return false;
    const api = await resolveWorkspaceApi(env, fetchImpl);
    if (!api)
        return undefined;
    const workspacePath = await canonicalPath(clientCwd);
    const body = await fetchJson(fetchImpl, `${api.externalApiUrl}/workspaces`, api.externalApiToken);
    const workspaces = responseDataRecord(body)?.workspaces;
    if (!Array.isArray(workspaces))
        return false;
    return workspaces.some((workspace) => {
        const record = asRecord(workspace);
        return record?.state === "active" && record.canonical_path === workspacePath;
    });
}
