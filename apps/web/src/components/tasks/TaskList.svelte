<script lang="ts">
  import { onMount } from 'svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import { loadTasks, selectedTaskId, selectTask, tasks, tasksError, tasksLoading } from '../../stores/tasks';

  onMount(() => {
    void loadTasks();
  });
</script>

<section class="panel">
  <div class="panel-heading">
    <h2>Tasks</h2>
    <button class="secondary" on:click={loadTasks}>Refresh</button>
  </div>
  <ErrorBanner message={$tasksError} />
  {#if $tasksLoading}
    <LoadingState message="Loading tasks..." />
  {:else if !$tasks.length}
    <EmptyState message="No tasks loaded." />
  {:else}
    <div class="list">
      {#each $tasks as task (task.task_id)}
        <button class:active={task.task_id === $selectedTaskId} class="item" on:click={() => selectTask(task.task_id)}>
          <strong>{task.input}</strong>
          <span>{task.state} · {task.routing_state}</span>
          <small class="muted">{task.task_id}</small>
        </button>
      {/each}
    </div>
  {/if}
</section>
