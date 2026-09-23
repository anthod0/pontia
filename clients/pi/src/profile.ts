import type { PiConnection } from "./control-socket.js";
import { asRecord, optionalString } from "./values.js";

export async function loadProfileSystemPrompt(
  connection: Pick<PiConnection, "request">,
  sessionId?: string,
): Promise<string | undefined> {
  if (!sessionId) return undefined;
  const sessionBody = await connection.request("session.get", { session_id: sessionId });
  const session = asRecord(asRecord(sessionBody)?.session);
  const profileId = optionalString(session?.execution_profile_id);
  const profileVersion = optionalString(session?.execution_profile_version);
  if (!profileId) return undefined;

  const profileBody = await connection.request("profile.get", {
    profile_id: profileId,
    ...(profileVersion ? { version: profileVersion } : {}),
  });
  const profile = asRecord(asRecord(profileBody)?.agent_profile);
  return optionalString(profile?.system_prompt_template);
}
