import type { EnvLike } from "./context.js";
import { resolvePontiaConnection } from "./discovery.js";
import { fetchJson, optionalString, responseDataRecord } from "./internal-api.js";

export async function loadProfileSystemPrompt(env: EnvLike, fetchImpl: typeof fetch): Promise<string | undefined> {
  const connection = await resolvePontiaConnection({ env, fetch: fetchImpl });
  const baseUrl = connection?.externalApiUrl;
  const token = connection?.externalApiToken;
  const sessionId = optionalString(env.PONTIA_SESSION_ID);
  if (!baseUrl || !token || !sessionId) return undefined;

  const sessionBody = await fetchJson(fetchImpl, `${baseUrl}/sessions/${encodeURIComponent(sessionId)}`, token);
  const session = responseDataRecord(sessionBody)?.session;
  if (!session || typeof session !== "object" || Array.isArray(session)) return undefined;
  const sessionRecord = session as Record<string, unknown>;
  const profileId = optionalString(sessionRecord.execution_profile_id);
  const profileVersion = optionalString(sessionRecord.execution_profile_version);
  if (!profileId) return undefined;

  const profileUrl = profileVersion
    ? `${baseUrl}/agent-profiles/${encodeURIComponent(profileId)}/versions/${encodeURIComponent(profileVersion)}`
    : `${baseUrl}/agent-profiles/${encodeURIComponent(profileId)}`;
  const profileBody = await fetchJson(fetchImpl, profileUrl, token);
  const profile = responseDataRecord(profileBody)?.agent_profile;
  if (!profile || typeof profile !== "object" || Array.isArray(profile)) return undefined;
  return optionalString((profile as Record<string, unknown>).system_prompt_template);
}
