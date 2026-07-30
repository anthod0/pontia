import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { EnvLike } from "./context.js";

const execFileAsync = promisify(execFile);

export async function isPontiaManagedTmuxPane(env: EnvLike = process.env): Promise<boolean> {
  const tmux = env.TMUX?.trim();
  const paneId = env.TMUX_PANE?.trim();
  const socketPath = tmux?.split(",", 1)[0]?.trim();
  if (!socketPath || !paneId) return false;

  try {
    const { stdout } = await execFileAsync("tmux", [
      "-S",
      socketPath,
      "show-options",
      "-p",
      "-v",
      "-t",
      paneId,
      "@pontia_session_id",
    ]);
    return stdout.trim().length > 0;
  } catch {
    return false;
  }
}
