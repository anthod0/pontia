import { createMutation, createQuery, queryOptions } from "@tanstack/svelte-query";
import {
  createAgentProfile as requestCreateAgentProfile,
  createAgentProfileVersion as requestCreateAgentProfileVersion,
  deleteAgentProfile as requestDeleteAgentProfile,
  deleteAgentProfileVersion as requestDeleteAgentProfileVersion,
  getAgentProfile,
  getAgentProfileVersion,
  listAgentProfiles,
  listAgentProfileVersions,
  updateAgentProfileVersion as requestUpdateAgentProfileVersion,
} from "../api/client";
import type { UpsertAgentProfileInput } from "../api/types";
import { queryClient } from "./queryClient";

export const agentProfileKeys = {
  all: ["agent-profiles"] as const,
  lists: () => [...agentProfileKeys.all, "list"] as const,
  list: (includeArchived: boolean) => [...agentProfileKeys.lists(), { includeArchived }] as const,
  details: () => [...agentProfileKeys.all, "detail"] as const,
  detail: (profileId: string) => [...agentProfileKeys.details(), profileId] as const,
  versions: (profileId: string) => [...agentProfileKeys.detail(profileId), "versions"] as const,
  versionList: (profileId: string, includeArchived: boolean) =>
    [...agentProfileKeys.versions(profileId), "list", { includeArchived }] as const,
  version: (profileId: string, version: string) =>
    [...agentProfileKeys.versions(profileId), version] as const,
};

function agentProfilesOptions(includeArchived: boolean) {
  return queryOptions({
    queryKey: agentProfileKeys.list(includeArchived),
    queryFn: ({ signal }) => listAgentProfiles(includeArchived, { signal }),
  });
}

function agentProfileVersionsOptions(
  profileId: string,
  includeArchived: boolean,
  enabled: boolean,
) {
  return queryOptions({
    queryKey: agentProfileKeys.versionList(profileId, includeArchived),
    enabled: enabled && profileId.length > 0,
    queryFn: ({ signal }) => listAgentProfileVersions(profileId, includeArchived, { signal }),
  });
}

function agentProfileOptions(profileId: string, enabled: boolean) {
  return queryOptions({
    queryKey: agentProfileKeys.detail(profileId),
    enabled: enabled && profileId.length > 0,
    queryFn: ({ signal }) => getAgentProfile(profileId, { signal }),
  });
}

function agentProfileVersionOptions(profileId: string, version: string, enabled: boolean) {
  return queryOptions({
    queryKey: agentProfileKeys.version(profileId, version),
    enabled: enabled && profileId.length > 0 && version.length > 0,
    queryFn: ({ signal }) => getAgentProfileVersion(profileId, version, { signal }),
  });
}

export function createAgentProfilesQuery(includeArchived: () => boolean = () => false) {
  return createQuery(
    () => agentProfilesOptions(includeArchived()),
    () => queryClient,
  );
}

export function createAgentProfileVersionsQuery(
  profileId: () => string,
  includeArchived: () => boolean = () => false,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => agentProfileVersionsOptions(profileId(), includeArchived(), enabled()),
    () => queryClient,
  );
}

export function createAgentProfileQuery(
  profileId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => agentProfileOptions(profileId(), enabled()),
    () => queryClient,
  );
}

export function createAgentProfileVersionQuery(
  profileId: () => string,
  version: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => agentProfileVersionOptions(profileId(), version(), enabled()),
    () => queryClient,
  );
}

function invalidateProfileLists(): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: agentProfileKeys.lists() });
}

function invalidateProfile(profileId: string): Promise<void[]> {
  return Promise.all([
    invalidateProfileLists(),
    queryClient.invalidateQueries({ queryKey: agentProfileKeys.detail(profileId) }),
  ]);
}

export function createAgentProfileMutation() {
  return createMutation(
    () => ({
      mutationFn: (input: UpsertAgentProfileInput) => requestCreateAgentProfile(input),
      onSuccess: async (profile) => {
        await invalidateProfile(profile.profile_id);
      },
    }),
    () => queryClient,
  );
}

export function createAgentProfileVersionMutation() {
  return createMutation(
    () => ({
      mutationFn: ({ profileId, input }: { profileId: string; input: UpsertAgentProfileInput }) =>
        requestCreateAgentProfileVersion(profileId, input),
      onSuccess: async (profile) => {
        await invalidateProfile(profile.profile_id);
      },
    }),
    () => queryClient,
  );
}

export function updateAgentProfileVersionMutation() {
  return createMutation(
    () => ({
      mutationFn: ({
        profileId,
        version,
        input,
      }: {
        profileId: string;
        version: string;
        input: UpsertAgentProfileInput;
      }) => requestUpdateAgentProfileVersion(profileId, version, input),
      onSuccess: async (profile) => {
        await invalidateProfile(profile.profile_id);
      },
    }),
    () => queryClient,
  );
}

export function deleteAgentProfileMutation() {
  return createMutation(
    () => ({
      mutationFn: (profileId: string) => requestDeleteAgentProfile(profileId),
      onSuccess: async (_result, profileId) => {
        await queryClient.cancelQueries({ queryKey: agentProfileKeys.detail(profileId) });
        queryClient.removeQueries({ queryKey: agentProfileKeys.detail(profileId) });
        await invalidateProfileLists();
      },
    }),
    () => queryClient,
  );
}

export function deleteAgentProfileVersionMutation() {
  return createMutation(
    () => ({
      mutationFn: ({ profileId, version }: { profileId: string; version: string }) =>
        requestDeleteAgentProfileVersion(profileId, version),
      onSuccess: async (profile) => {
        await invalidateProfile(profile.profile_id);
      },
    }),
    () => queryClient,
  );
}
