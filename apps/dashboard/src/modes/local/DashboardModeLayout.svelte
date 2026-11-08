<script lang="ts">
  import { onDestroy, onMount, type Snippet } from 'svelte';
  import { get } from 'svelte/store';
  import AuthGate from '../../components/auth/AuthGate.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import DeviceProvider from '../../device/DeviceProvider.svelte';
  import { DeviceRuntime } from '../../device/runtime';
  import { createApiAccess } from './apiAccess';
  import { consumeTokenFromUrl, loadTokenFromStorage, token } from './auth';

  let { children, handle: _handle }: { children: Snippet; handle?: string } = $props();
  let unsubscribeToken: (() => void) | undefined;
  loadTokenFromStorage();
  consumeTokenFromUrl();
  let device = $state.raw(new DeviceRuntime('local', createApiAccess()));

  onMount(() => {
    let previous = get(token).trim();
    unsubscribeToken = token.subscribe((value) => {
      const credential = value.trim();
      if (credential !== previous) {
        previous = credential;
        device.dispose();
        device = new DeviceRuntime('local', createApiAccess());
      }
      if (credential) device.lifecycle.startDashboardRuntime();
    });
  });

  onDestroy(() => {
    unsubscribeToken?.();
    device.dispose();
  });
</script>

{#key device}
  <DeviceProvider {device}>
    {#if $token.trim()}
      <AppShell>{@render children()}</AppShell>
    {:else}
      <AuthGate />
    {/if}
  </DeviceProvider>
{/key}
