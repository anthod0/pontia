<script lang="ts">
  import EmptyState from '../common/EmptyState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import JsonView from '../common/JsonView.svelte';
  import {
    artifactContent,
    artifactContentError,
    artifactContentLoading,
    selectedArtifact,
  } from '../../stores/artifacts';

  const TEXT_CONTENT_TYPES = [
    'application/json',
    'application/xml',
    'application/javascript',
    'application/x-ndjson',
    'application/yaml',
    'application/toml',
  ];

  function isTextContent(contentType: string): boolean {
    const normalized = contentType.toLowerCase().split(';', 1)[0].trim();
    return normalized.startsWith('text/') || TEXT_CONTENT_TYPES.includes(normalized) || normalized.endsWith('+json') || normalized.endsWith('+xml');
  }

  $: contentIsText = $artifactContent ? isTextContent($artifactContent.contentType) : false;
  $: contentSize = $artifactContent ? $artifactContent.bytes.byteLength : 0;
</script>

<div class="artifact-content">
  <h3>Artifact content</h3>
  <ErrorBanner message={$artifactContentError} />
  {#if $artifactContentLoading}
    <LoadingState message="Loading artifact content..." />
  {:else if !$selectedArtifact}
    <EmptyState message="Select an artifact to read its content." />
  {:else if !$artifactContent}
    <EmptyState message="No content loaded yet." />
  {:else}
    <div class="metadata-grid compact-grid">
      <span>Content type</span><strong>{$artifactContent.contentType}</strong>
      <span>Size</span><strong>{contentSize} bytes</strong>
    </div>
    {#if contentIsText}
      <pre class="artifact-pre">{$artifactContent.text}</pre>
    {:else}
      <p class="muted">
        This artifact was returned as <code>{$artifactContent.contentType}</code>, so the dashboard is not rendering it inline.
        Use the External API content endpoint to download or inspect the raw bytes.
      </p>
      <JsonView value={{ artifact_id: $artifactContent.artifactId, content_type: $artifactContent.contentType, size_bytes: contentSize }} />
    {/if}
  {/if}
</div>
