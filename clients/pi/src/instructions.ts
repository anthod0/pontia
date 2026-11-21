import type { RuntimeConnection } from "../shared/control-socket.js";
import { asRecord, optionalString } from "./values.js";

export async function loadSessionSystemPrompt(
  connection: Pick<RuntimeConnection, "request">,
  sessionId?: string,
): Promise<string | undefined> {
  if (!sessionId) return undefined;
  const sessionBody = await connection.request("session.get", { session_id: sessionId });
  const binding = asRecord(asRecord(sessionBody)?.session_instructions);
  return binding ? optionalString(binding.system_prompt) : undefined;
}
