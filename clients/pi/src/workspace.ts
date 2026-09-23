import { realpath } from "node:fs/promises";
import { resolve } from "node:path";
import type { PiConnection } from "./control-socket.js";
import { asRecord } from "./values.js";

async function canonicalPath(path: string): Promise<string> {
  try {
    return await realpath(path);
  } catch {
    return resolve(path);
  }
}

export async function isActiveRegisteredWorkspace(connection: Pick<PiConnection, "request">, clientCwd: string | undefined): Promise<boolean> {
  if (!clientCwd) return false;

  const workspacePath = await canonicalPath(clientCwd);
  const body = await connection.request("workspaces.list", {});
  const workspaces = asRecord(body)?.workspaces;
  if (!Array.isArray(workspaces)) return false;

  return workspaces.some((workspace) => {
    const record = asRecord(workspace);
    return record?.state === "active" && record.canonical_path === workspacePath;
  });
}
