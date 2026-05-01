<script lang="ts">
  import { get } from 'svelte/store';
  import { interruptSession, restartSession, terminateSession } from '../../api/client';
  import { selectedSessionId, selectSession } from '../../stores/selection';
  import { loadSessions } from '../../stores/sessions';
  import { setStatus } from '../../stores/ui';

  async function run(label: string, action: (id: string) => Promise<unknown>) {
    const id = get(selectedSessionId);
    if (!id) return setStatus('Select a session first.', true);
    try {
      await action(id);
      await Promise.all([loadSessions(), selectSession(id)]);
      setStatus(`Session ${label} requested.`);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    }
  }
</script>

<section class="panel">
  <h2>Lifecycle actions</h2>
  <div class="row">
    <button class="secondary" on:click={() => run('interrupt', interruptSession)}>Interrupt</button>
    <button class="secondary" on:click={() => run('restart', restartSession)}>Restart</button>
    <button class="danger" on:click={() => run('terminate', terminateSession)}>Terminate</button>
  </div>
</section>
