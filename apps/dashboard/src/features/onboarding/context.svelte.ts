import { getContext, setContext } from "svelte";

const contextKey = Symbol("agent-client-onboarding");

export class AgentClientOnboardingStatus {
  probing = $state(false);
}

export function provideAgentClientOnboardingStatus(): AgentClientOnboardingStatus {
  const status = new AgentClientOnboardingStatus();
  setContext(contextKey, status);
  return status;
}

export function useAgentClientOnboardingStatus(): AgentClientOnboardingStatus | undefined {
  return getContext<AgentClientOnboardingStatus>(contextKey);
}
