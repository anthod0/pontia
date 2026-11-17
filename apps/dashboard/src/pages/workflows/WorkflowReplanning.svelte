<script lang="ts">
  import { statusLabel } from '$lib/i18n/status';
  import { t, currentLocale } from '$lib/i18n';
  import { formatDateTime } from '$lib/format';
  import { useDeviceRuntime } from '../../device/context';
  import { untrack } from 'svelte'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { navigate } from '$lib/navigation'
  import type { WorkflowDetailView } from '../../api/types'
  import WorkflowDocument from './WorkflowDocument.svelte'
  import WorkflowSession from './WorkflowSession.svelte'
  const device = useDeviceRuntime();
  const { selectedWorkflowHistorySessionIds } = device.workflows;
  const { createWorkflowPatchesQuery } = device.workflowQueries;

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
  function time(value: string | null): string { return value ? formatDateTime(value, $currentLocale) : $t('common.notRecorded') }
</script>

{#if patchesQuery.error}
  <Alert.Root variant="destructive"><Alert.Title>{$t('workflow.replanningFailed')}</Alert.Title><Alert.Description>{patchesQuery.error.message}</Alert.Description></Alert.Root>
  <Button variant="outline" onclick={() => void patchesQuery.refetch()}>{$t('workflow.retryRecords')}</Button>
{/if}
{#if patchesQuery.isFetching && patchesQuery.data === undefined}<p role="status">{$t('workflow.loadingReplanning')}</p>{/if}
{#if patches.length}
  <section class="space-y-3" aria-label={$t('workflow.replanningRecords')}>
    <h3 class="font-semibold">{$t('workflow.replanningVersion')}{revision}</h3>
    {#each patches as patch (snapshot.workflow_id + ':' + patch.patch_id)}
      <Card.Root class="gap-5 p-4 md:p-6" role="region" aria-label={$t('workflow.patchNamed', { id: patch.patch_id })}>
        <div class="flex flex-wrap items-center gap-2"><h4 class="break-all text-lg font-semibold">{$t('workflow.patch')} {patch.patch_id}</h4><Badge variant="outline">{patch.patch_id === snapshot.active_patch?.patch_id ? $t('workflow.currentRequest') : $t('workflow.historicalRequest')}</Badge></div>
        <section class="space-y-2" aria-label={$t('workflow.request')}>
          <h5 class="font-semibold">{$t('workflow.requestStep')}</h5>
          <p class="text-sm">{$t('workflow.requestedAt')} {time(patch.requested_at)} {$t('workflow.baseVersion')}{patch.base_revision}</p>
          <p class="break-all text-sm">{$t('workflow.fromNode')} {patch.requesting_node_id}</p>
          <div class="flex flex-wrap items-center gap-2 text-sm"><span class="break-all">{$t('workflow.requestingSession')} {patch.requesting_session_id}</span>{#if patch.requesting_session_id}<Button variant="outline" size="sm" onclick={() => navigate(`/chat/${encodeURIComponent(patch.requesting_session_id)}`)}>{$t('workflow.openRequestingChat')}</Button>{/if}</div>
          <p class="break-all text-sm">{$t('workflow.requestingTurn')} {patch.requesting_turn_id || $t('common.notRecorded')}</p>
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.request_document_ref} label={$t('workflow.requestDocument')} />
        </section>
        <section class="space-y-2" aria-label={$t('workflow.replanner')}>
          <h5 class="font-semibold">{$t('workflow.replannerStep')}</h5>
          {#if patch.replanner_session_id}
            <WorkflowSession sessionId={patch.replanner_session_id} {snapshot} />
            <p class="text-xs text-muted-foreground">{$t('workflow.currentSessionHint')}</p>
          {:else if patch.patch_id === snapshot.active_patch?.patch_id}
            <p class="text-sm text-muted-foreground">{$t('workflow.waitingReplanner')}</p>
          {:else}<p class="text-sm text-muted-foreground">{$t('workflow.noReplanner')}</p>{/if}
          <p class="break-all text-sm">{$t('workflow.replannerTurn')} {patch.replanner_turn_id ?? $t('common.notRecorded')}</p>
          <p class="text-sm">{$t('workflow.planning')} {time(patch.planning_at)}</p>
        </section>
        <section class="space-y-2" aria-label={$t('workflow.patchOutcome')}>
          <h5 class="font-semibold">{$t('workflow.patchStep')}</h5>
          <p class="text-sm">{$t('workflow.patchState')} {statusLabel(patch.state, $t)} {$t('workflow.outcome')} {patch.outcome ? statusLabel(patch.outcome, $t) : $t('common.notRecorded')}</p>
          <p class="text-sm">{$t('workflow.resolvedAt')} {time(patch.resolved_at)}</p>
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.decision_document_ref} label={$t('workflow.decisionDocument')} />
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.reason_document_ref} label={$t('workflow.reasonDocument')} />
          <WorkflowDocument workflowId={snapshot.workflow_id} documentRef={patch.blocked_draft_ref} label={$t('workflow.blockedDraft')} />
          <p class="break-all text-sm">{$t('workflow.addedNodes')} {patch.added_node_ids.join(', ') || $t('common.noneRecorded')}</p>
          <p class="break-all text-sm">{$t('workflow.retiredNodes')} {patch.retired_node_ids.join(', ') || $t('common.noneRecorded')}</p>
        </section>
        <section class="space-y-2" aria-label={$t('workflow.resultingRevision')}>
          <h5 class="font-semibold">{$t('workflow.revisionStep')}</h5>
          {#if patch.result_revision !== null && patch.result_revision > patch.base_revision}
            <Button variant="outline" onclick={() => patch.result_revision !== null && onrevision(patch.result_revision)}>{$t('workflow.viewRevisionPrefix')}{patch.base_revision} → v{patch.result_revision}</Button>
          {:else if patch.result_revision === patch.base_revision}<p class="text-sm">{$t('workflow.noNewRevisionPrefix')}{patch.base_revision}.</p>
          {:else}<p class="text-sm">{$t('workflow.noResultingRevisionPrefix')}{snapshot.current_revision}.</p>{/if}
        </section>
      </Card.Root>
    {/each}
  </section>
{/if}
