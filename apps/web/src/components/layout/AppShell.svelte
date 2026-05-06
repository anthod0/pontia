<script lang="ts">
  import StatusBar from './StatusBar.svelte';
  import Sidebar from './Sidebar.svelte';
  import TaskDetail from '../tasks/TaskDetail.svelte';
  import SessionDetail from '../sessions/SessionDetail.svelte';
  import SessionActions from '../sessions/SessionActions.svelte';
  import TurnComposer from '../turns/TurnComposer.svelte';
  import LatestReply from '../turns/LatestReply.svelte';
  import TurnHistory from '../turns/TurnHistory.svelte';
  import EventTimeline from '../events/EventTimeline.svelte';
  import ArtifactBrowser from '../artifacts/ArtifactBrowser.svelte';
</script>

<StatusBar />
<main>
  <Sidebar />
  <section class="content">
    <TaskDetail />
    <SessionDetail />
    <SessionActions />
    <TurnComposer />
    <LatestReply />
    <TurnHistory />
    <EventTimeline />
    <ArtifactBrowser />
  </section>
</main>
