import { mkdir, appendFile } from "node:fs/promises";
import { dirname } from "node:path";
export async function appendDiagnostic(logFile, entry) {
    try {
        await mkdir(dirname(logFile), { recursive: true });
        await appendFile(logFile, `${JSON.stringify({ time: new Date().toISOString(), ...entry })}\n`, "utf8");
    }
    catch {
        // Diagnostics must never affect Claude Code hook execution.
    }
}
