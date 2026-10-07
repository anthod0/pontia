import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { EnvLike } from "./context.js";

const execFileAsync = promisify(execFile);

export interface ManagedRuntimeIdentity {
  sessionId: string;
  runtimeId: string;
}

export function hasTmuxPaneEnvironment(env: EnvLike = process.env): boolean {
  const socketPath = env.TMUX?.trim().split(",", 1)[0]?.trim();
  return Boolean(socketPath && env.TMUX_PANE?.trim());
}

async function paneOption(
  socketPath: string,
  paneId: string,
  option: string,
): Promise<string | undefined> {
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
  if (!hasTmuxPaneEnvironment(env)) return undefined;
  const socketPath = env.TMUX!.trim().split(",", 1)[0]!.trim();
  const paneId = env.TMUX_PANE!.trim();

  const [sessionId, runtimeId] = await Promise.all([
    paneOption(socketPath, paneId, "@pontia_session_id"),
    paneOption(socketPath, paneId, "@pontia_runtime_id"),
  ]);
  if (!sessionId || !runtimeId) return undefined;
  return { sessionId, runtimeId };
}

export async function isPontiaManagedTmuxPane(env: EnvLike = process.env): Promise<boolean> {
  return (await loadPontiaManagedRuntimeIdentity(env)) !== undefined;
}
