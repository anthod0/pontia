<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { setStatus } from '../../stores/ui';
  import {
    browseWorkspaceRoot,
    loadWorkspaceRoots,
    loadWorkspaces,
    registerWorkspace,
    workspaceRoots,
    workspaces,
  } from '../../stores/workspaces';
  import type { WorkspaceDirectoryListingView, WorkspaceView } from '../../api/types';

  export let selectedWorkspaceId = '';
  export let label = 'Workspace';

  const dispatch = createEventDispatcher<{
    selected: WorkspaceView | null;
  }>();

  let registering = false;
  let browserOpen = false;
  let selectedRootId = '';
  let listing: WorkspaceDirectoryListingView | null = null;
  let selectedPath = '';
  let workspaceName = '';

  onMount(async () => {
    await loadWorkspaces();
    emitSelection();
  });

  $: selectedWorkspace = $workspaces.find((workspace) => workspace.workspace_id === selectedWorkspaceId) ?? null;

  function emitSelection() {
    const workspace = $workspaces.find((item) => item.workspace_id === selectedWorkspaceId) ?? null;
    dispatch('selected', workspace);
  }

  async function openBrowser() {
    browserOpen = true;
    try {
      const roots = await loadWorkspaceRoots();
      selectedRootId = selectedRootId || roots.find((root) => root.state === 'available')?.root_id || roots[0]?.root_id || '';
      if (selectedRootId) await browse('');
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    }
  }

  async function browse(path: string) {
    if (!selectedRootId) return;
    try {
      listing = await browseWorkspaceRoot(selectedRootId, path);
      selectedPath = listing.path;
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    }
  }

  async function registerSelectedWorkspace() {
    if (!selectedRootId) return;
    registering = true;
    try {
      const workspace = await registerWorkspace({
        root_id: selectedRootId,
        path: selectedPath,
        name: workspaceName.trim() || null,
      });
      selectedWorkspaceId = workspace.workspace_id;
      browserOpen = false;
      workspaceName = '';
      dispatch('selected', workspace);
      setStatus('Workspace registered.');
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    } finally {
      registering = false;
    }
  }
</script>

<label>
  {label}
  <select bind:value={selectedWorkspaceId} on:change={emitSelection}>
    <option value="">Select a known workspace</option>
    {#each $workspaces as workspace (workspace.workspace_id)}
      <option value={workspace.workspace_id}>{workspace.name ?? workspace.display_path}</option>
    {/each}
  </select>
</label>
{#if selectedWorkspace}
  <small class="muted">{selectedWorkspace.canonical_path}</small>
{/if}
<button class="secondary" type="button" on:click={openBrowser}>Register workspace</button>

{#if browserOpen}
  <div class="nested-panel">
    {#if !$workspaceRoots.length}
      <p class="muted">No workspace roots configured. Set LLMPARTY_WORKSPACE_ROOTS to enable browsing.</p>
    {:else}
      <label>
        Root
        <select bind:value={selectedRootId} on:change={() => browse('')}>
          {#each $workspaceRoots as root (root.root_id)}
            <option value={root.root_id} disabled={root.state !== 'available'}>{root.label} · {root.state}</option>
          {/each}
        </select>
      </label>
      {#if listing}
        <small class="muted">{listing.canonical_path}</small>
        <div class="row">
          <button class="secondary" type="button" disabled={listing.parent_path === null} on:click={() => browse(listing?.parent_path ?? '')}>Up</button>
          <button class="secondary" type="button" on:click={() => { selectedPath = listing?.path ?? ''; }}>Use this directory</button>
        </div>
        <div class="list compact">
          {#each listing.entries as entry (entry.path)}
            <button class="item" type="button" on:click={() => browse(entry.path)}>
              <strong>{entry.name}</strong>
              <span>{entry.is_workspace ? 'known workspace' : entry.path}</span>
            </button>
          {/each}
        </div>
        <label>Display name <input bind:value={workspaceName} placeholder="Optional" /></label>
        <button disabled={registering} type="button" on:click={registerSelectedWorkspace}>{registering ? 'Registering...' : 'Register selected directory'}</button>
      {/if}
    {/if}
  </div>
{/if}
