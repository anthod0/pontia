<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { token } from '../../stores/auth';
  import { lastConnectionError, sseStatus } from '../../stores/connection';
  import { startEventStream, stopEventStream } from '../../services/eventStream';
  import StatusBar from './StatusBar.svelte';
  import Sidebar from './Sidebar.svelte';
  import SessionDetail from '../sessions/SessionDetail.svelte';
  import SessionActions from '../sessions/SessionActions.svelte';
  import TurnComposer from '../turns/TurnComposer.svelte';
  import LatestReply from '../turns/LatestReply.svelte';
  import TurnHistory from '../turns/TurnHistory.svelte';
  import EventTimeline from '../events/EventTimeline.svelte';
  import ArtifactBrowser from '../artifacts/ArtifactBrowser.svelte';
  import TaskDetail from '../tasks/TaskDetail.svelte';

  let dashboardView = 'sessions' as 'sessions' | 'tasks';
  let unsubscribeToken: (() => void) | null = null;

  onMount(() => {
    unsubscribeToken = token.subscribe((value) => {
      stopEventStream();
      if (value.trim()) {
        startEventStream();
      } else {
        sseStatus.set('idle');
        lastConnectionError.set('Set an API token to open the dashboard event stream.');
      }
    });
  });

  onDestroy(() => {
    unsubscribeToken?.();
    stopEventStream();
  });
</script>

<StatusBar />
<div class="view-switcher" aria-label="Dashboard view">
  <label><input type="radio" bind:group={dashboardView} value="sessions" /> Sessions</label>
  <label><input type="radio" bind:group={dashboardView} value="tasks" /> Tasks / DAG</label>
</div>
<main>
  <Sidebar view={dashboardView} />
  <section class="content">
    {#if dashboardView === 'tasks'}
      <TaskDetail />
    {:else}
      <SessionDetail />
      <SessionActions />
      <TurnComposer />
      <LatestReply />
      <TurnHistory />
      <EventTimeline />
      <ArtifactBrowser />
    {/if}
  </section>
</main>
