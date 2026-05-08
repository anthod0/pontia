<script lang="ts">
  import { createSession } from '../../api/client';
  import { loadSessions } from '../../stores/sessions';
  import { selectSession } from '../../stores/selection';
  import { setStatus } from '../../stores/ui';

  let clientType = 'claude_code';
  let workspace = '.';
  let initialTask = '';
  let creating = false;

  async function submit() {
    creating = true;
    try {
      const result = await createSession({
        client_type: clientType,
        workspace,
        initial_task: initialTask.trim() ? { input: initialTask.trim() } : null,
      });
      await loadSessions();
      await selectSession(result.session.session_id);
      initialTask = '';
      setStatus('Session created.');
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    } finally {
      creating = false;
    }
  }
</script>

<section class="panel">
  <h2>Create session</h2>
  <label>Client type <select bind:value={clientType}><option value="claude_code">claude_code</option><option value="pi">pi</option></select></label>
  <label>Workspace <input bind:value={workspace} placeholder="/path/to/workspace" /></label>
  <label>Initial task <textarea bind:value={initialTask} placeholder="Optional initial task"></textarea></label>
  <button disabled={creating} on:click={submit}>{creating ? 'Creating...' : 'Create session'}</button>
</section>
