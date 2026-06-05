import { mkdir, appendFile } from "node:fs/promises";
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
    await appendFile(logFile, `${JSON.stringify({ time: new Date().toISOString(), ...entry })}\n`, "utf8");
  } catch (error) {
    console.error("pilotfy Claude Code plugin diagnostic write failed", error);
  }
}
