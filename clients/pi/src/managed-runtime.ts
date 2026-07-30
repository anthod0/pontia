import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { EnvLike } from "./context.js";

const execFileAsync = promisify(execFile);

export interface ManagedRuntimeIdentity {
  sessionId: string;
  runtimeInstanceId: string;
}

async function paneOption(socketPath: string, paneId: string, option: string): Promise<string | undefined> {
  try {
    const { stdout } = await execFileAsync("tmux", [
      "-S",
      socketPath,
      "show-options",
      "-p",
      "-v",
      "-t",
      paneId,
      option,
    ]);
    return stdout.trim() || undefined;
  } catch {
    return undefined;
  }
}

export async function loadPontiaManagedRuntimeIdentity(
  env: EnvLike = process.env,
): Promise<ManagedRuntimeIdentity | undefined> {
  const tmux = env.TMUX?.trim();
  const paneId = env.TMUX_PANE?.trim();
  const socketPath = tmux?.split(",", 1)[0]?.trim();
  if (!socketPath || !paneId) return undefined;

  const [sessionId, runtimeInstanceId] = await Promise.all([
    paneOption(socketPath, paneId, "@pontia_session_id"),
    paneOption(socketPath, paneId, "@pontia_runtime_instance_id"),
  ]);
  if (!sessionId || !runtimeInstanceId) return undefined;
  return { sessionId, runtimeInstanceId };
}

export async function isPontiaManagedTmuxPane(env: EnvLike = process.env): Promise<boolean> {
  return (await loadPontiaManagedRuntimeIdentity(env)) !== undefined;
}
