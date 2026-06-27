import { writable } from 'svelte/store';
import { listAgentProfiles, type ReadRequestOptions } from '../api/client';
import type { AgentProfileView } from '../api/types';

const FALLBACK_CLIENT_TYPES = ['pi'];

export const agentProfiles = writable<AgentProfileView[]>([]);
export const agentProfilesLoading = writable(false);
export const agentProfilesError = writable<string | null>(null);

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError';
}

export async function loadAgentProfiles(includeArchived = false, options: ReadRequestOptions = {}): Promise<void> {
  agentProfilesLoading.set(true);
  agentProfilesError.set(null);
  try {
    agentProfiles.set(await listAgentProfiles(includeArchived, options));
  } catch (error) {
    if (!isAbortError(error)) agentProfilesError.set(error instanceof Error ? error.message : String(error));
  } finally {
    agentProfilesLoading.set(false);
  }
}

export function clientTypeOptionsForProfile(profile: AgentProfileView | null): string[] {
  const profileOptions = profile?.supported_client_types.filter((client) => client !== 'generic') ?? [];
  return profileOptions.length ? profileOptions : FALLBACK_CLIENT_TYPES;
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
