import type {
  AgentClientInstallMethod,
  AgentClientOperationState,
  AgentClientView,
} from "../../api/types";
import { readSseFrames } from "../../services/readSse";

export type OperationStreamEvent =
  | { event: "state" | "done"; operation: AgentClientOperationState; log_size: number }
  | { event: "log"; offset: number; next_offset: number; chunk: string };

export function preferredInstallMethod(client: AgentClientView): AgentClientInstallMethod | null {
  const methods = client.install?.methods ?? [];
  return methods.find((method) => method.recommended) ?? methods[0] ?? null;
}

export function missingRequired(method: AgentClientInstallMethod) {
  return method.prerequisites.filter(
    (prerequisite) => prerequisite.required && !prerequisite.satisfied,
  );
}

export function optionalWarnings(method: AgentClientInstallMethod) {
  return method.prerequisites.filter(
    (prerequisite) => !prerequisite.required && !prerequisite.satisfied,
  );
}

export function lastOutputLine(output: string): string {
  return (
    output
      .split(/[\r\n]+/)
      .map((line) => line.trim())
      .filter(Boolean)
      .at(-1) ?? ""
  );
}

export async function readOperationStream(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: OperationStreamEvent) => void,
): Promise<void> {
  return readSseFrames(body, ({ event, data }) => {
    if (!["state", "log", "done"].includes(event)) return;
    onEvent({ event, ...JSON.parse(data) } as OperationStreamEvent);
  });
}
