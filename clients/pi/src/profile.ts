import type { PiConnection } from "./control-socket.js";
import { asRecord, optionalString } from "./values.js";

export async function loadProfileSystemPrompt(
  connection: Pick<PiConnection, "request">,
  sessionId?: string,
  existingMessages: readonly unknown[] = [],
): Promise<string | undefined> {
  if (!sessionId) return undefined;
  const sessionBody = await connection.request("session.get", { session_id: sessionId });
  const binding = asRecord(asRecord(sessionBody)?.execution_profile_binding);
  if (binding) return optionalString(binding.system_prompt);
  let previous: string | undefined;
  for (const value of existingMessages) {
    const message = asRecord(value);
    if (message?.role !== "system") continue;
    const sections = asRecord(message.sections);
    if (sections && "pontia_execution_profile" in sections) {
      previous = optionalString(sections.pontia_execution_profile);
    }
  }
  return previous;
}
