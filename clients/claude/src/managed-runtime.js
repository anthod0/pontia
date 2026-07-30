import { execFile } from "node:child_process";
import { promisify } from "node:util";
const execFileAsync = promisify(execFile);
async function paneOption(socketPath, paneId, option) {
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
    }
    catch {
        return undefined;
    }
}
export async function loadPontiaManagedRuntimeIdentity(env = process.env) {
    const tmux = env.TMUX?.trim();
    const paneId = env.TMUX_PANE?.trim();
    const socketPath = tmux?.split(",", 1)[0]?.trim();
    if (!socketPath || !paneId)
        return undefined;
    const [sessionId, runtimeInstanceId] = await Promise.all([
        paneOption(socketPath, paneId, "@pontia_session_id"),
        paneOption(socketPath, paneId, "@pontia_runtime_instance_id"),
    ]);
    if (!sessionId || !runtimeInstanceId)
        return undefined;
    return { sessionId, runtimeInstanceId };
}
export async function isPontiaManagedTmuxPane(env = process.env) {
    return (await loadPontiaManagedRuntimeIdentity(env)) !== undefined;
}
