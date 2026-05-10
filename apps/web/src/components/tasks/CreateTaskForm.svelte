<script lang="ts">
  import { createTask } from '../../stores/tasks';
  import { loadSessions } from '../../stores/sessions';
  import { selectSession } from '../../stores/selection';
  import { loadWorkspaces } from '../../stores/workspaces';
  import { setStatus } from '../../stores/ui';
  import WorkspaceSelector from '../workspaces/WorkspaceSelector.svelte';
  import type { WorkspaceView } from '../../api/types';

  let clientType = 'claude_code';
  let workspaceId = '';
  let workspacePath = '';
  let input = '';
  let creating = false;

  function handleWorkspaceSelected(event: CustomEvent<WorkspaceView | null>) {
    workspacePath = event.detail?.canonical_path ?? '';
  }

  async function submit() {
    const taskInput = input.trim();
    if (!taskInput) return;
    creating = true;
    try {
      const task = await createTask({
        input: taskInput,
        client_type: clientType,
        workspace: workspacePath || null,
      });
      await Promise.all([loadSessions(), loadWorkspaces()]);
      if (task.session_id) await selectSession(task.session_id);
      input = '';
      setStatus(task.session_id ? 'Task created and dispatched.' : 'Task created; workspace confirmation may be required.');
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    } finally {
      creating = false;
    }
  }
</script>

<section class="panel">
  <h2>Create task</h2>
  <p class="muted">Use the current control-plane task API. Leave workspace empty to validate manual routing / confirmation.</p>
  <label>Client type <select bind:value={clientType}><option value="claude_code">claude_code</option><option value="pi">pi</option></select></label>
  <WorkspaceSelector bind:selectedWorkspaceId={workspaceId} on:selected={handleWorkspaceSelected} />
  <label>Task <textarea bind:value={input} placeholder="Ask the agent control layer to do work"></textarea></label>
  <button disabled={creating || !input.trim()} on:click={submit}>{creating ? 'Creating...' : 'Create task'}</button>
</section>
