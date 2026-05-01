<script lang="ts">
  import JsonView from '../common/JsonView.svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import { session, sessionError, sessionLoading } from '../../stores/sessionDetail';
</script>

<section class="panel">
  <h2>Selected session</h2>
  <ErrorBanner message={$sessionError} />
  {#if $sessionLoading}
    <LoadingState message="Loading session..." />
  {:else if $session}
    <JsonView value={$session} />
  {:else}
    <EmptyState message="Select a session." />
  {/if}
</section>
