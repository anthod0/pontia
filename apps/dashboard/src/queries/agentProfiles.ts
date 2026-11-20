import { createMutation, createQuery } from "@tanstack/svelte-query";
import type { UpsertAgentProfileInput } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export function createAgentProfileQueries(device: DeviceRuntime) {
  const agentProfileKeys = { all: [device.id, "agent-profiles"] as const };
  const queryKey = agentProfileKeys.all;
  const invalidate = () => device.queryClient.invalidateQueries({ queryKey });

  function createAgentProfilesQuery() {
    return createQuery(
      () => ({ queryKey, queryFn: ({ signal }) => device.api.listAgentProfiles({ signal }) }),
      () => device.queryClient,
    );
  }

  function createAgentProfileMutation() {
    return createMutation(
      () => ({
        mutationFn: (input: UpsertAgentProfileInput) => device.api.createAgentProfile(input),
        onSuccess: invalidate,
      }),
      () => device.queryClient,
    );
  }

  function updateAgentProfileMutation() {
    return createMutation(
      () => ({
        mutationFn: (input: UpsertAgentProfileInput) =>
          device.api.updateAgentProfile(input.profile_id, input),
        onSuccess: invalidate,
      }),
      () => device.queryClient,
    );
  }

  function deleteAgentProfileMutation() {
    return createMutation(
      () => ({
        mutationFn: (profileId: string) => device.api.deleteAgentProfile(profileId),
        onSuccess: invalidate,
      }),
      () => device.queryClient,
    );
  }

  return {
    agentProfileKeys,
    createAgentProfilesQuery,
    createAgentProfileMutation,
    updateAgentProfileMutation,
    deleteAgentProfileMutation,
  };
}
