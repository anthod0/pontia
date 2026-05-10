<script lang="ts">
  import { createSession } from '../../api/client';
  import { loadSessions } from '../../stores/sessions';
  import { selectSession } from '../../stores/selection';
  import { setStatus } from '../../stores/ui';
  import WorkspaceSelector from '../workspaces/WorkspaceSelector.svelte';

  let clientType = 'claude_code';
  let workspaceId = '';
  let handle = '';
  let initialTask = '';
  let creating = false;

  async function submit() {
    creating = true;
    try {
      const result = await createSession({
        client_type: clientType,
        workspace_id: workspaceId || null,
        handle: handle.trim() || null,
        initial_task: initialTask.trim() ? { input: initialTask.trim() } : null,
      });
      await loadSessions();
      await selectSession(result.session.session_id);
      handle = '';
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
  <WorkspaceSelector bind:selectedWorkspaceId={workspaceId} />

  <label>Handle <input bind:value={handle} placeholder="@reviewer" /></label>
  <label>Initial task <textarea bind:value={initialTask} placeholder="Optional initial task"></textarea></label>
  <button disabled={creating || !workspaceId} on:click={submit}>{creating ? 'Creating...' : 'Create session'}</button>
</section>
