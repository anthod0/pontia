import { mkdir, open } from "node:fs/promises";
import { dirname } from "node:path";

export type DiagnosticLevel = "info" | "warn" | "error";

export interface DiagnosticEntry {
  level: DiagnosticLevel;
  code: string;
  message: string;
  details?: unknown;
}

export async function appendDiagnostic(logFile: string, entry: DiagnosticEntry): Promise<void> {
  try {
    await mkdir(dirname(logFile), { recursive: true });
    const file = await open(logFile, "a", 0o600);
    try {
      if (process.platform !== "win32") await file.chmod(0o600);
      await file.writeFile(
        `${JSON.stringify({ time: new Date().toISOString(), ...entry })}\n`,
        "utf8",
      );
    } finally {
      await file.close();
    }
  } catch (error) {
    // Diagnostics must never break pi execution.
    console.error("pontia pi plugin diagnostic write failed", error);
  }
}
