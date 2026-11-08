import { createMutation, createQuery, queryOptions } from "@tanstack/svelte-query";

import type { UpsertAgentProfileInput } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createAgentProfileQueries(device: DeviceRuntime) {
  const agentProfileKeys = {
    all: [device.id, "agent-profiles"] as const,
    lists: () => [...agentProfileKeys.all, "list"] as const,
    list: (includeArchived: boolean) => [...agentProfileKeys.lists(), { includeArchived }] as const,
    details: () => [...agentProfileKeys.all, "detail"] as const,
    detail: (profileId: string) => [...agentProfileKeys.details(), profileId] as const,
    versions: (profileId: string) => [...agentProfileKeys.detail(profileId), "versions"] as const,
    versionList: (profileId: string, includeArchived: boolean) =>
      [...agentProfileKeys.versions(profileId), "list", { includeArchived }] as const,
  };

  function agentProfilesOptions(includeArchived: boolean) {
    return queryOptions({
      queryKey: agentProfileKeys.list(includeArchived),
      queryFn: ({ signal }) => device.api.listAgentProfiles(includeArchived, { signal }),
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
      queryFn: ({ signal }) =>
        device.api.listAgentProfileVersions(profileId, includeArchived, { signal }),
    });
  }

  function createAgentProfilesQuery(includeArchived: () => boolean = () => false) {
    return createQuery(
      () => agentProfilesOptions(includeArchived()),
      () => device.queryClient,
    );
  }

  function createAgentProfileVersionsQuery(
    profileId: () => string,
    includeArchived: () => boolean = () => false,
    enabled: () => boolean = () => true,
  ) {
    return createQuery(
      () => agentProfileVersionsOptions(profileId(), includeArchived(), enabled()),
      () => device.queryClient,
    );
  }

  function invalidateProfileLists(): Promise<void> {
    return device.queryClient.invalidateQueries({ queryKey: agentProfileKeys.lists() });
  }

  function invalidateProfile(profileId: string): Promise<void[]> {
    return Promise.all([
      invalidateProfileLists(),
      device.queryClient.invalidateQueries({ queryKey: agentProfileKeys.detail(profileId) }),
    ]);
  }

  function createAgentProfileMutation() {
    return createMutation(
      () => ({
        mutationFn: (input: UpsertAgentProfileInput) => device.api.createAgentProfile(input),
        onSuccess: async (profile) => {
          await invalidateProfile(profile.profile_id);
        },
      }),
      () => device.queryClient,
    );
  }

  function createAgentProfileVersionMutation() {
    return createMutation(
      () => ({
        mutationFn: ({ profileId, input }: { profileId: string; input: UpsertAgentProfileInput }) =>
          device.api.createAgentProfileVersion(profileId, input),
        onSuccess: async (profile) => {
          await invalidateProfile(profile.profile_id);
        },
      }),
      () => device.queryClient,
    );
  }

  function updateAgentProfileVersionMutation() {
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
        }) => device.api.updateAgentProfileVersion(profileId, version, input),
        onSuccess: async (profile) => {
          await invalidateProfile(profile.profile_id);
        },
      }),
      () => device.queryClient,
    );
  }

  function deleteAgentProfileMutation() {
    return createMutation(
      () => ({
        mutationFn: (profileId: string) => device.api.deleteAgentProfile(profileId),
        onSuccess: async (_result, profileId) => {
          await device.queryClient.cancelQueries({ queryKey: agentProfileKeys.detail(profileId) });
          device.queryClient.removeQueries({ queryKey: agentProfileKeys.detail(profileId) });
          await invalidateProfileLists();
        },
      }),
      () => device.queryClient,
    );
  }

  function deleteAgentProfileVersionMutation() {
    return createMutation(
      () => ({
        mutationFn: ({ profileId, version }: { profileId: string; version: string }) =>
          device.api.deleteAgentProfileVersion(profileId, version),
        onSuccess: async (profile) => {
          await invalidateProfile(profile.profile_id);
        },
      }),
      () => device.queryClient,
    );
  }
  return {
    agentProfileKeys,
    createAgentProfilesQuery,
    createAgentProfileVersionsQuery,
    createAgentProfileMutation,
    createAgentProfileVersionMutation,
    updateAgentProfileVersionMutation,
    deleteAgentProfileMutation,
    deleteAgentProfileVersionMutation,
  };
}
