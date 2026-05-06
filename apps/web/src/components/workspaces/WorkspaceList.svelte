<script lang="ts">
  import { onMount } from 'svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import { loadWorkspaces, workspaces, workspacesError, workspacesLoading } from '../../stores/workspaces';

  onMount(() => {
    void loadWorkspaces();
  });
</script>

<section class="panel">
  <div class="panel-heading">
    <h2>Workspaces</h2>
    <button class="secondary" on:click={loadWorkspaces}>Refresh</button>
  </div>
  <ErrorBanner message={$workspacesError} />
  {#if $workspacesLoading}
    <LoadingState message="Loading workspaces..." />
  {:else if !$workspaces.length}
    <EmptyState message="No known workspaces." />
  {:else}
    <div class="list">
      {#each $workspaces as workspace (workspace.workspace_id)}
        <div class="item passive">
          <strong>{workspace.name ?? workspace.display_path}</strong>
          <span>{workspace.state} · {workspace.workspace_id}</span>
          <small class="muted">{workspace.canonical_path}</small>
        </div>
      {/each}
    </div>
  {/if}
</section>
