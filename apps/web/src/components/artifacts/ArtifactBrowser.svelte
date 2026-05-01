<script lang="ts">
  import EmptyState from '../common/EmptyState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import JsonView from '../common/JsonView.svelte';
  import ArtifactContentViewer from './ArtifactContentViewer.svelte';
  import type { ArtifactView } from '../../api/types';
  import {
    artifacts,
    artifactsError,
    artifactsLoading,
    discoverArtifacts,
    loadArtifactContent,
    selectedArtifact,
  } from '../../stores/artifacts';
  import { selectedSessionId } from '../../stores/selection';

  let actionError: string | null = null;
  let discovering = false;

  async function discover(): Promise<void> {
    if (!$selectedSessionId) return;
    discovering = true;
    actionError = null;
    try {
      await discoverArtifacts($selectedSessionId);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      discovering = false;
    }
  }

  async function selectArtifact(artifact: ArtifactView): Promise<void> {
    actionError = null;
    try {
      await loadArtifactContent(artifact);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    }
  }

  function formatSize(bytes: number | null): string {
    if (bytes === null || Number.isNaN(bytes)) return 'unknown size';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  }
</script>

<section class="panel">
  <div class="panel-heading">
    <h2>Artifacts</h2>
    <button class="secondary" type="button" disabled={!$selectedSessionId || discovering} on:click={discover}>
      {discovering ? 'Discovering...' : 'Discover artifacts'}
    </button>
  </div>
  <ErrorBanner message={$artifactsError ?? actionError} />
  {#if !$selectedSessionId}
    <EmptyState message="Select a session to browse artifacts." />
  {:else if $artifactsLoading}
    <LoadingState message="Loading artifacts..." />
  {:else if !$artifacts.length}
    <EmptyState message="No artifacts indexed for this session. Use discover to rescan the session workspace." />
  {:else}
    <div class="artifact-layout">
      <div class="list artifact-list" aria-label="Artifacts">
        {#each $artifacts as artifact (artifact.artifact_id)}
          <button
            type="button"
            class:item={true}
            class:active={$selectedArtifact?.artifact_id === artifact.artifact_id}
            on:click={() => selectArtifact(artifact)}
          >
            <span class="row">
              <strong>{artifact.name}</strong>
              <span class="pill">{artifact.kind}</span>
            </span>
            <span class="muted">{formatSize(artifact.size_bytes)} · {artifact.artifact_id}</span>
            {#if artifact.preview}
              <span class="artifact-preview">{artifact.preview}</span>
            {/if}
          </button>
        {/each}
      </div>
      <div class="artifact-detail">
        {#if $selectedArtifact}
          <h3>Metadata</h3>
          <JsonView value={$selectedArtifact} />
        {/if}
        <ArtifactContentViewer />
      </div>
    </div>
  {/if}
</section>
