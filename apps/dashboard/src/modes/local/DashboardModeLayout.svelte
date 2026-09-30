<script lang="ts">
  import { onDestroy, onMount, type Snippet } from 'svelte';
  import { get } from 'svelte/store';
  import AuthGate from '../../components/auth/AuthGate.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import { startEventStream, stopEventStream } from '../../services/eventStream';
  import { loadAgentProfiles } from '../../stores/agentProfiles';
  import { consumeTokenFromUrl, loadTokenFromStorage, token } from '$dashboard-mode/auth';
  import { loadSessions } from '../../stores/sessions';
  import { loadTasks } from '../../stores/tasks';
  import { loadWorkspaces } from '../../stores/workspaces';
  import { loadWorkflows } from '../../stores/workflows';

  let { children, handle: _handle }: { children: Snippet; handle?: string } = $props();
  let unsubscribeToken: (() => void) | null = null;
  let dashboardStarted = false;
  loadTokenFromStorage();
  let authenticatedToken = $state(get(token).trim());

  function startDashboard(): void {
    void Promise.all([loadTasks(), loadWorkspaces(), loadAgentProfiles(), loadSessions(), loadWorkflows({ showLoading: false })]);
    startEventStream();
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
          stopEventStream();
          dashboardStarted = false;
        }
        return;
      }
      if (!dashboardStarted) {
        startDashboard();
        return;
      }
      if (trimmed !== previousToken) {
        stopEventStream();
        startEventStream();
      }
    });
  });

  onDestroy(() => {
    unsubscribeToken?.();
    stopEventStream();
  });
</script>

{#if authenticatedToken}
  <AppShell>{@render children()}</AppShell>
{:else}
  <AuthGate />
{/if}
