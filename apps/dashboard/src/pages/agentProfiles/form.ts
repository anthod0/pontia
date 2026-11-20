import { translate } from "$lib/i18n";
import type { AgentProfileView, UpsertAgentProfileInput } from "../../api/types";

export interface AgentProfileDraft {
  profile_id: string;
  name: string;
  description: string;
  client_type: string;
  client_key: string;
  model: string;
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
    system_prompt: profile.system_prompt ?? "",
  };
}

export function buildAgentProfileInput(
  draft: AgentProfileDraft,
): { ok: true; input: UpsertAgentProfileInput } | { ok: false; errors: AgentProfileDraftErrors } {
  const errors: AgentProfileDraftErrors = {};
  if (!draft.profile_id.trim()) errors.profile_id = translate("profiles.idRequired");
  if (!draft.name.trim()) errors.name = translate("profiles.nameRequired");
  const launch = [draft.client_type, draft.client_key, draft.model].map((value) => value.trim());
  if (launch.some(Boolean) && !launch.every(Boolean))
    errors.model = translate("profiles.clientModelRequired");
  if (draft.system_prompt.includes("{{") || draft.system_prompt.includes("}}"))
    errors.system_prompt = translate("profiles.staticPromptRequired");
  if (Object.keys(errors).length) return { ok: false, errors };
  return {
    ok: true,
    input: {
      profile_id: draft.profile_id.trim(),
      name: draft.name.trim(),
      description: draft.description.trim() || null,
      client_type: launch[0] || null,
      client_key: launch[1] || null,
      model: launch[2] || null,
      system_prompt: draft.system_prompt.trim() || null,
    },
  };
}
