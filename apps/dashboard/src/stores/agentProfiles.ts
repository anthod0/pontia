import { writable } from 'svelte/store';
import { listAgentProfiles } from '../api/client';
import type { AgentProfileView } from '../api/types';

const FALLBACK_CLIENT_TYPES = ['claude_code', 'pi', 'generic'];

export const agentProfiles = writable<AgentProfileView[]>([]);
export const agentProfilesLoading = writable(false);
export const agentProfilesError = writable<string | null>(null);

export async function loadAgentProfiles(): Promise<void> {
  agentProfilesLoading.set(true);
  agentProfilesError.set(null);
  try {
    agentProfiles.set(await listAgentProfiles());
  } catch (error) {
    agentProfilesError.set(error instanceof Error ? error.message : String(error));
  } finally {
    agentProfilesLoading.set(false);
  }
}

export function clientTypeOptionsForProfile(profile: AgentProfileView | null): string[] {
  return profile?.supported_client_types.length ? profile.supported_client_types : FALLBACK_CLIENT_TYPES;
}

export function selectClientTypeForProfile(currentClientType: string, profile: AgentProfileView | null): string {
  const options = clientTypeOptionsForProfile(profile);
  return options.includes(currentClientType) ? currentClientType : options[0] ?? currentClientType;
}

export function defaultHandleForProfile(profile: AgentProfileView | null): string {
  const prefix = profile?.handle_prefix?.trim();
  if (!prefix) return '';
  return prefix.startsWith('@') ? prefix : `@${prefix}`;
}

export function sessionProfileFields(profile: AgentProfileView | null): {
  execution_profile_id?: string;
  execution_profile_version?: string;
} {
  if (!profile) return {};
  return {
    execution_profile_id: profile.profile_id,
    execution_profile_version: profile.version,
  };
}
