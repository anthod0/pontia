<script lang="ts">
  import JsonView from '../common/JsonView.svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import {
    cancelTask,
    confirmTaskWorkspace,
    createHumanSignal,
    interruptTask,
    pauseTask,
    refreshTask,
    resumeTask,
    selectedTaskId,
    submitPlannerInput,
    task,
    taskDag,
    taskError,
    taskEvents,
    taskLoading,
  } from '../../stores/tasks';
  import { loadSessions } from '../../stores/sessions';
  import { selectSession } from '../../stores/selection';
  import { loadWorkspaces } from '../../stores/workspaces';
  import { setStatus } from '../../stores/ui';
  import WorkspaceSelector from '../workspaces/WorkspaceSelector.svelte';
  import type { WorkItemRunView, WorkspaceView } from '../../api/types';

  let workspaceId = '';
  let workspacePath = '';
  let plannerMessage = '';
  let clientType = 'claude_code';
  let signalKind = 'user_objection';
  let signalSummary = '';
  let signalDetail = '';
  let signalSeverity: 'low' | 'medium' | 'high' = 'medium';
  let busy = false;

  $: canConfirm = $task?.state === 'needs_confirmation';
  $: canCancel = $task && !['completed', 'failed', 'cancelled'].includes($task.state);
  $: canPause = $task && !['paused', 'completed', 'failed', 'cancelled'].includes($task.state);
  $: canResume = $task?.state === 'paused';
  $: canInterrupt = $task && ['queued', 'running'].includes($task.state);

  function handleWorkspaceSelected(event: CustomEvent<WorkspaceView | null>) {
    workspacePath = event.detail?.canonical_path ?? '';
  }

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

  function latestRunFor(workItemId: string): WorkItemRunView | undefined {
    return $taskDag?.runs.filter((run) => run.work_item_id === workItemId).at(-1);
  }

  async function openRun(run: WorkItemRunView) {
    if (!run.session_id) return;
    await selectSession(run.session_id);
  }

  async function submitHumanSignal() {
    if (!$task || !signalSummary.trim()) return;
    await createHumanSignal($task.task_id, {
      kind: signalKind.trim() || 'user_objection',
      summary: signalSummary.trim(),
      detail: signalDetail.trim() || null,
      severity: signalSeverity,
    });
    signalSummary = '';
    signalDetail = '';
    setStatus('Human signal submitted.');
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
      <WorkspaceSelector bind:selectedWorkspaceId={workspaceId} label="Workspace confirmation" on:selected={handleWorkspaceSelected} />
      <button disabled={busy || !workspacePath} on:click={() => run(async () => { await confirmTaskWorkspace($task!.task_id, { workspace: workspacePath, client_type: clientType }); await syncDispatchedSession(); workspaceId = ''; workspacePath = ''; setStatus('Workspace confirmed.'); })}>Confirm workspace</button>
    {/if}

    <label>Planner input <textarea bind:value={plannerMessage} placeholder="Optional message for planner / routing"></textarea></label>
    <button class="secondary" disabled={busy || !plannerMessage.trim()} on:click={() => run(async () => { await submitPlannerInput($task!.task_id, { message: plannerMessage.trim(), client_type: clientType }); await syncDispatchedSession(); plannerMessage = ''; setStatus('Planner input submitted.'); })}>Submit planner input</button>

    <div class="row task-actions">
      <button class="secondary" disabled={!$task.session_id} on:click={() => $task?.session_id && selectSession($task.session_id)}>Open session</button>
      <button class="secondary" disabled={busy || !canPause} on:click={() => run(async () => { await pauseTask($task!.task_id); setStatus('Task paused.'); })}>Pause</button>
      <button class="secondary" disabled={busy || !canResume} on:click={() => run(async () => { await resumeTask($task!.task_id); setStatus('Task resumed.'); })}>Resume</button>
      <button class="secondary" disabled={busy || !canInterrupt} on:click={() => run(async () => { await interruptTask($task!.task_id); setStatus('Task interrupt requested.'); })}>Interrupt task</button>
      <button class="danger" disabled={busy || !canCancel} on:click={() => run(async () => { await cancelTask($task!.task_id); setStatus('Task cancelled.'); })}>Cancel task</button>
    </div>

    <form class="human-signal" on:submit|preventDefault={() => run(submitHumanSignal)}>
      <h3>Human signal</h3>
      <div class="row">
        <label>Kind <input bind:value={signalKind} placeholder="user_objection" /></label>
        <label>Severity
          <select bind:value={signalSeverity}>
            <option value="low">low</option>
            <option value="medium">medium</option>
            <option value="high">high</option>
          </select>
        </label>
      </div>
      <label>Summary <input bind:value={signalSummary} placeholder="Plan is too broad" /></label>
      <label>Detail <textarea bind:value={signalDetail} placeholder="Optional detail for the planner or operator"></textarea></label>
      <button class="secondary" disabled={busy || !signalSummary.trim()}>Submit signal</button>
    </form>

    <details>
      <summary>Task JSON</summary>
      <JsonView value={$task} />
    </details>

    <h3>DAG</h3>
    {#if !$taskDag || $taskDag.summary.total_work_items === 0}
      <EmptyState message="No DAG work items yet." />
    {:else}
      <div class="metadata-grid compact-grid">
        <span>WorkItems</span><strong>{$taskDag.summary.total_work_items}</strong>
        <span>Ready</span><strong>{$taskDag.summary.ready_work_items}</strong>
        <span>Running</span><strong>{$taskDag.summary.running_work_items}</strong>
        <span>Completed</span><strong>{$taskDag.summary.completed_work_items}</strong>
        <span>Blocked</span><strong>{$taskDag.summary.blocked_work_items}</strong>
        <span>Failed</span><strong>{$taskDag.summary.failed_work_items}</strong>
        <span>Runs</span><strong>{$taskDag.summary.total_runs}</strong>
        <span>Open signals</span><strong>{$taskDag.summary.open_signals}</strong>
      </div>

      <div class="timeline compact">
        {#each $taskDag.work_items as item (item.work_item_id)}
          {@const run = latestRunFor(item.work_item_id)}
          <article class="timeline-item">
            <div class="row"><strong>{item.title}</strong><span class="badge">{item.runtime?.current_state ?? 'unknown'}</span></div>
            <p class="muted">{item.kind} · {item.execution_profile_id}{#if item.execution_profile_version}@{item.execution_profile_version}{/if}</p>
            {#if item.description}<p>{item.description}</p>{/if}
            {#if run}
              <div class="metadata-grid compact-grid">
                <span>Run</span><strong>{run.run_id} ({run.state})</strong>
                <span>Session</span><strong>{run.session_id ?? 'none'}</strong>
                <span>Turn</span><strong>{run.turn_id ?? 'none'}</strong>
              </div>
              <button class="secondary" disabled={!run.session_id} on:click={() => openRun(run)}>Open run session</button>
            {/if}
          </article>
        {/each}
      </div>

      <h4>Signals</h4>
      {#if !$taskDag.signals.length}
        <EmptyState message="No DAG signals." />
      {:else}
        <div class="timeline compact">
          {#each $taskDag.signals as signal (signal.signal_id)}
            <article class="timeline-item">
              <div class="row"><strong>{signal.kind}</strong><span class="badge">{signal.severity} · {signal.state}</span></div>
              <p>{signal.summary}</p>
              {#if signal.detail}<p class="muted">{signal.detail}</p>{/if}
              <p class="muted">Source {signal.source} · Run {signal.run_id ?? 'none'} · WorkItem {signal.work_item_id ?? 'none'}</p>
            </article>
          {/each}
        </div>
      {/if}
    {/if}

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
