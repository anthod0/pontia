<script lang="ts">
  import { untrack, type Snippet } from 'svelte';
  import { provideDeviceRuntime } from './context';
  import type { DeviceRuntime } from './runtime';
  import AgentClientOnboardingHost from '../features/onboarding/AgentClientOnboardingHost.svelte';
  import { provideAgentClientOnboardingStatus } from '../features/onboarding/context.svelte';
  let { device, children, onboarding = false }: { device: DeviceRuntime; children: Snippet; onboarding?: boolean } = $props();
  provideDeviceRuntime(untrack(() => device));
  provideAgentClientOnboardingStatus();
</script>

{@render children()}
{#if onboarding}<AgentClientOnboardingHost />{/if}
