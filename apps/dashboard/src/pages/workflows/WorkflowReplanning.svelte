<script lang="ts">
  import { untrack } from 'svelte'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { navigate } from '$lib/navigation'
  import type { WorkflowDetailView } from '../../api/types'
  import { selectedWorkflowHistorySessionIds } from '../../stores/workflows'
  import { createWorkflowPatchesQuery } from '../../queries/workflows'
  import WorkflowDocument from './WorkflowDocument.svelte'
  import WorkflowSession from './WorkflowSession.svelte'

  let { snapshot, revision, onrevision }: {
    snapshot: WorkflowDetailView; revision: number;
    onrevision: (revision: number) => void;
  } = $props()
  const patchesQuery = createWorkflowPatchesQuery(() => snapshot.workflow_id, () => false)
  let workflowId = $derived(snapshot.workflow_id)
  $effect(() => { workflowId; return () => { selectedWorkflowHistorySessionIds.set([]) } })
  $effect(() => { snapshot; untrack(() => void patchesQuery.refetch()) })
  let patches = $derived((patchesQuery.data ?? []).filter(patch => patch.base_revision === revision))
  $effect(() => { selectedWorkflowHistorySessionIds.set(patches.flatMap(patch => patch.replanner_session_id ? [patch.replanner_session_id] : [])) })
  function time(value: string | null): string { return value ?? 'Not recorded' }
</script>

{#if patchesQuery.error}
  <Alert.Root variant="destructive"><Alert.Title>Could not load replanning records</Alert.Title><Alert.Description>{patchesQuery.error.message}</Alert.Description></Alert.Root>
  <Button variant="outline" onclick={() => void patchesQuery.refetch()}>Retry records</Button>
{/if}
{#if patchesQuery.isFetching && patchesQuery.data === undefined}<p role="status">Loading replanning records…</p>{/if}
{#if patches.length}
  <section class="space-y-3" aria-label="Replanning records">
    <h3 class="font-semibold">Replanning records · v{revision}</h3>
    {#each patches as patch (snapshot.workflow_id + ':' + patch.patch_id)}
      <Card.Root class="gap-5 p-4 md:p-6" role="region" aria-label={`Patch ${patch.patch_id}`}>
        <div class="flex flex-wrap items-center gap-2"><h4 class="break-all text-lg font-semibold">Patch {patch.patch_id}</h4><Badge variant="outline">{patch.patch_id === snapshot.active_patch?.patch_id ? 'Current request' : 'Historical request'}</Badge></div>
        <section class="space-y-2" aria-label="Request">
          <h5 class="font-semibold">1. Request</h5>
          <p class="text-sm">Requested: {time(patch.requested_at)} · Base v{patch.base_revision}</p>
          <p class="break-all text-sm">From node: {patch.requesting_node_id}</p>
          <div class="flex flex-wrap items-center gap-2 text-sm"><span class="break-all">Requesting Session: {patch.requesting_session_id}</span>{#if patch.requesting_session_id}<Button variant="outline" size="sm" onclick={() => navigate(`/chat/${encodeURIComponent(patch.requesting_session_id)}`)}>Open requesting chat</Button>{/if}</div>
          <p class="break-all text-sm">Requesting Turn: {patch.requesting_turn_id || 'Not recorded'}</p>
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.request_document_ref} label="Request document" />
        </section>
        <section class="space-y-2" aria-label="Replanner">
          <h5 class="font-semibold">2. Replanner</h5>
          {#if patch.replanner_session_id}
            <WorkflowSession sessionId={patch.replanner_session_id} {snapshot} />
            <p class="text-xs text-muted-foreground">Session status is current, not a historical status snapshot.</p>
          {:else if patch.patch_id === snapshot.active_patch?.patch_id}
            <p class="text-sm text-muted-foreground">Waiting for Replanner Session creation.</p>
          {:else}<p class="text-sm text-muted-foreground">No associated Replanner Session recorded.</p>{/if}
          <p class="break-all text-sm">Replanner Turn: {patch.replanner_turn_id ?? 'Not recorded'}</p>
          <p class="text-sm">Planning: {time(patch.planning_at)}</p>
        </section>
        <section class="space-y-2" aria-label="Patch outcome">
          <h5 class="font-semibold">3. Patch</h5>
          <p class="text-sm">Patch state: {patch.state} · Outcome: {patch.outcome ?? 'Not recorded'}</p>
          <p class="text-sm">Resolved: {time(patch.resolved_at)}</p>
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.decision_document_ref} label="Decision document" />
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.reason_document_ref} label="Reason document" />
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.blocked_draft_ref} label="Blocked draft" />
          <p class="break-all text-sm">Added nodes: {patch.added_node_ids.join(', ') || 'None recorded'}</p>
          <p class="break-all text-sm">Retired nodes: {patch.retired_node_ids.join(', ') || 'None recorded'}</p>
        </section>
        <section class="space-y-2" aria-label="Resulting revision">
          <h5 class="font-semibold">4. Revision</h5>
          {#if patch.result_revision !== null && patch.result_revision > patch.base_revision}
            <Button variant="outline" onclick={() => patch.result_revision !== null && onrevision(patch.result_revision)}>View revision v{patch.base_revision} → v{patch.result_revision}</Button>
          {:else if patch.result_revision === patch.base_revision}<p class="text-sm">No new revision. Version remained v{patch.base_revision}.</p>
          {:else}<p class="text-sm">No resulting revision recorded. Current effective version: v{snapshot.current_revision}.</p>{/if}
        </section>
      </Card.Root>
    {/each}
  </section>
{/if}
