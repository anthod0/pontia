<script lang="ts">
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
