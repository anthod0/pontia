import { clientDisplayName } from "./clientName";

export function untitledSessionLabel(clientType: string | null | undefined): string {
  const normalized = clientType?.trim();
  const name = normalized === "claude-agent" ? "Claude Code" : clientDisplayName(normalized ?? "");
  return `Untitled ${name || "agent"} session`;
}
