<script lang="ts">
  import { onDestroy, onMount, type Snippet } from 'svelte';
  import { get } from 'svelte/store';
  import AuthGate from '../../components/auth/AuthGate.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import { startEventStream } from '../../services/eventStream';
  import { startDashboardRuntime, stopDashboardRuntime } from '../../services/dashboardRuntime';
  import { consumeTokenFromUrl, loadTokenFromStorage, token } from '$dashboard-mode/auth';

  let { children, handle: _handle }: { children: Snippet; handle?: string } = $props();
  let unsubscribeToken: (() => void) | null = null;
  let dashboardStarted = false;
  loadTokenFromStorage();
  let authenticatedToken = $state(get(token).trim());

  function startDashboard(): void {
    startDashboardRuntime();
    dashboardStarted = true;
  }

  onMount(() => {
    consumeTokenFromUrl();
    unsubscribeToken = token.subscribe((value) => {
      const trimmed = value.trim();
      const previousToken = authenticatedToken;
      authenticatedToken = trimmed;
      if (!trimmed) {
        if (dashboardStarted) {
          stopDashboardRuntime();
          dashboardStarted = false;
        }
        return;
      }
      if (!dashboardStarted) {
        startDashboard();
        return;
      }
      if (trimmed !== previousToken) {
        stopDashboardRuntime();
        startEventStream();
      }
    });
  });

  onDestroy(() => {
    unsubscribeToken?.();
    stopDashboardRuntime();
  });
</script>

{#if authenticatedToken}
  <AppShell>{@render children()}</AppShell>
{:else}
  <AuthGate />
{/if}
