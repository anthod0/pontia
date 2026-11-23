import { translate } from "$lib/i18n";
import type { AgentProfileView, UpsertAgentProfileInput } from "../../api/types";

export interface AgentProfileDraft {
  profile_id: string;
  name: string;
  description: string;
  client_type: string;
  client_key: string;
  model: string;
  reasoning_effort: string;
  system_prompt: string;
}

export type AgentProfileDraftErrors = Partial<Record<keyof AgentProfileDraft, string>>;

export function createAgentProfileDraft(): AgentProfileDraft {
  return {
    profile_id: "",
    name: "",
    description: "",
    client_type: "",
    client_key: "",
    model: "",
    reasoning_effort: "",
    system_prompt: "",
  };
}

export function createAgentProfileDraftFromProfile(profile: AgentProfileView): AgentProfileDraft {
  return {
    profile_id: profile.profile_id,
    name: profile.name,
    description: profile.description ?? "",
    client_type: profile.client_type ?? "",
    client_key: profile.client_key ?? "",
    model: profile.model ?? "",
    reasoning_effort: profile.reasoning_effort ?? "",
    system_prompt: profile.system_prompt ?? "",
  };
}

/** Matches the format pontiad accepts for new Profile IDs. */
const profileIdPattern = /^[a-z0-9][a-z0-9._-]*$/;

/** Suggests a Profile ID from its name; names without ASCII letters or digits give "". */
export function profileIdFromName(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function buildAgentProfileInput(
  draft: AgentProfileDraft,
  { creating }: { creating: boolean },
): { ok: true; input: UpsertAgentProfileInput } | { ok: false; errors: AgentProfileDraftErrors } {
  const errors: AgentProfileDraftErrors = {};
  // An existing ID is the resource path, so it is sent back exactly as stored.
  const profileId = creating ? draft.profile_id.trim() : draft.profile_id;
  if (!profileId) errors.profile_id = translate("profiles.idRequired");
  else if (creating && !profileIdPattern.test(profileId))
    errors.profile_id = translate("profiles.idInvalid");
  if (!draft.name.trim()) errors.name = translate("profiles.nameRequired");
  const launch = [draft.client_type, draft.client_key, draft.model].map((value) => value.trim());
  if (!launch.every(Boolean)) errors.model = translate("profiles.clientModelRequired");
  if (draft.system_prompt.includes("{{") || draft.system_prompt.includes("}}"))
    errors.system_prompt = translate("profiles.staticPromptRequired");
  if (Object.keys(errors).length) return { ok: false, errors };
  return {
    ok: true,
    input: {
      profile_id: profileId,
      name: draft.name.trim(),
      description: draft.description.trim() || null,
      client_type: launch[0],
      client_key: launch[1],
      model: launch[2],
      reasoning_effort: draft.reasoning_effort || null,
      system_prompt: draft.system_prompt.trim() || null,
    },
  };
}
