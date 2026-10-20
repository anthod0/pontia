<script lang="ts">
  import {
    createSessionOverviewQuery,
    snapshotSessionOverview,
    type SessionOverviewSnapshot,
  } from '../../src/queries/sessionOverview';

  let {
    includeArchived = false,
    workspaceId,
    onSnapshot,
  }: {
    includeArchived?: boolean;
    workspaceId?: string;
    onSnapshot: (snapshot: SessionOverviewSnapshot) => void;
  } = $props();

  // svelte-ignore state_referenced_locally -- the harness fixes this option for its lifetime
  const query = createSessionOverviewQuery(includeArchived, () => workspaceId);
  const snapshot = $derived(snapshotSessionOverview(query.data));

  $effect(() => {
    onSnapshot(snapshot);
  });
</script>

<button type="button" data-testid="load-more-sessions" onclick={() => void query.fetchNextPage()}>
  Load more sessions
</button>
