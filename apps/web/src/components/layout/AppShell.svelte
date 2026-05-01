<script lang="ts">
  import StatusBar from './StatusBar.svelte';
  import Sidebar from './Sidebar.svelte';
  import SessionDetail from '../sessions/SessionDetail.svelte';
  import SessionActions from '../sessions/SessionActions.svelte';
</script>

<StatusBar />
<main>
  <Sidebar />
  <section class="content">
    <SessionDetail />
    <SessionActions />
    <section class="panel placeholder">
      <h2>Turns, event timeline, and artifact browser</h2>
      <p>Session A wires the Svelte shell, API client, stores, session management, and lifecycle actions. Polling turns/events and artifacts UI follow in later sessions.</p>
    </section>
  </section>
</main>
