<script lang="ts">
  import JsonView from '../common/JsonView.svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import {
    cancelTask,
    confirmTaskWorkspace,
    interruptTask,
    refreshTask,
    selectedTaskId,
    submitPlannerInput,
    task,
    taskError,
    taskEvents,
    taskLoading,
  } from '../../stores/tasks';
  import { loadSessions } from '../../stores/sessions';
  import { selectSession } from '../../stores/selection';
  import { loadWorkspaces } from '../../stores/workspaces';
  import { setStatus } from '../../stores/ui';

  let workspace = '';
  let plannerMessage = '';
  let clientType = 'generic';
  let busy = false;

  $: canConfirm = $task?.state === 'needs_confirmation';
  $: canCancel = $task && !['completed', 'failed', 'cancelled'].includes($task.state);
  $: canInterrupt = $task && ['queued', 'running'].includes($task.state);

  async function run(action: () => Promise<void>) {
    busy = true;
    try {
      await action();
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    } finally {
      busy = false;
    }
  }

  async function syncDispatchedSession() {
    await Promise.all([loadSessions(), loadWorkspaces()]);
    if ($task?.session_id) await selectSession($task.session_id);
  }
</script>

<section class="panel">
  <div class="panel-heading">
    <h2>Selected task</h2>
    {#if $selectedTaskId}<button class="secondary" on:click={() => refreshTask($selectedTaskId!)}>Refresh</button>{/if}
  </div>
  <ErrorBanner message={$taskError} />
  {#if $taskLoading}
    <LoadingState message="Loading task..." />
  {:else if $task}
    <div class="metadata-grid compact-grid">
      <span>Task</span><strong>{$task.task_id}</strong>
      <span>State</span><strong>{$task.state}</strong>
      <span>Routing</span><strong>{$task.routing_state}{#if $task.routing_confidence != null} ({Math.round($task.routing_confidence * 100)}%){/if}</strong>
      <span>Session</span><strong>{$task.session_id ?? 'not dispatched'}</strong>
      <span>Turn</span><strong>{$task.turn_id ?? 'not dispatched'}</strong>
    </div>

    {#if canConfirm}
      <label>Workspace confirmation <input bind:value={workspace} placeholder="/path/to/workspace" /></label>
      <button disabled={busy || !workspace.trim()} on:click={() => run(async () => { await confirmTaskWorkspace($task!.task_id, { workspace: workspace.trim(), client_type: clientType }); await syncDispatchedSession(); workspace = ''; setStatus('Workspace confirmed.'); })}>Confirm workspace</button>
    {/if}

    <label>Planner input <textarea bind:value={plannerMessage} placeholder="Optional message for planner / routing"></textarea></label>
    <button class="secondary" disabled={busy || !plannerMessage.trim()} on:click={() => run(async () => { await submitPlannerInput($task!.task_id, { message: plannerMessage.trim(), client_type: clientType }); await syncDispatchedSession(); plannerMessage = ''; setStatus('Planner input submitted.'); })}>Submit planner input</button>

    <div class="row task-actions">
      <button class="secondary" disabled={!$task.session_id} on:click={() => $task?.session_id && selectSession($task.session_id)}>Open session</button>
      <button class="secondary" disabled={busy || !canInterrupt} on:click={() => run(async () => { await interruptTask($task!.task_id); setStatus('Task interrupt requested.'); })}>Interrupt task</button>
      <button class="danger" disabled={busy || !canCancel} on:click={() => run(async () => { await cancelTask($task!.task_id); setStatus('Task cancelled.'); })}>Cancel task</button>
    </div>

    <details>
      <summary>Task JSON</summary>
      <JsonView value={$task} />
    </details>

    <h3>Task events</h3>
    {#if !$taskEvents.length}
      <EmptyState message="No task events." />
    {:else}
      <div class="timeline compact">
        {#each $taskEvents as event (event.event_id)}
          <article class="timeline-item">
            <div class="row"><strong>{event.event_type}</strong><span class="muted">{event.created_at}</span></div>
            <JsonView value={event.payload} />
          </article>
        {/each}
      </div>
    {/if}
  {:else}
    <EmptyState message="Select a task." />
  {/if}
</section>
