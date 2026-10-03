import { isEdgePort } from "../../../../shared/edge-port";

export function deploymentModeCommand(
  command: string,
  mode: "default" | "custom",
  port: number | undefined,
): string | null {
  if (mode === "default") return command;
  if (!isEdgePort(port)) return null;
  return `${command} \\\n  --port ${port} \\\n  --acme-challenge dns-01`;
}
