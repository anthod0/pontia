<script lang="ts">
  import { onMount } from 'svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import { sessions, sessionsError, sessionsLoading, loadSessions } from '../../stores/sessions';
  import { visibleSessions } from '../../stores/sessionFilters';
  import { selectedSessionId, selectSession } from '../../stores/selection';

  onMount(() => {
    void loadSessions();
  });

  $: displaySessions = visibleSessions($sessions);
</script>

<section class="panel">
  <div class="panel-heading">
    <h2>Sessions</h2>
    <button class="secondary" on:click={loadSessions}>Refresh</button>
  </div>
  <ErrorBanner message={$sessionsError} />
  {#if $sessionsLoading}
    <LoadingState message="Loading sessions..." />
  {:else if !displaySessions.length}
    <EmptyState message="No active sessions loaded." />
  {:else}
    <div class="list">
      {#each displaySessions as session (session.session_id)}
        <button class:active={session.session_id === $selectedSessionId} class="item" on:click={() => selectSession(session.session_id)}>
          <strong>{session.session_id}</strong>
          <span>{session.client_type} · {session.state}</span>
        </button>
      {/each}
    </div>
  {/if}
</section>
