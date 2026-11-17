<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { navigate } from '$lib/navigation'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { Separator } from '$lib/components/ui/separator/index.js'
  const device = useDeviceRuntime();
  const { createWorkflowRevisionQuery } = device.workflowQueries;

  let { workflowId, revision, requestedPhase = null, onphase }: {
    workflowId: string; revision: number | null; requestedPhase?: string | null;
    onphase: (ordinal: number) => void;
  } = $props()
  const revisionQuery = createWorkflowRevisionQuery(() => workflowId, () => revision)
  let phases = $derived.by(() => {
    const groups: { ordinal: number; name: string; nodes: NonNullable<typeof revisionQuery.data>['nodes'] }[] = []
    for (const node of revisionQuery.data?.nodes ?? []) {
      let phase = groups.at(-1)
      if (!phase || phase.name !== node.phase) {
        phase = { ordinal: groups.length + 1, name: node.phase, nodes: [] }
        groups.push(phase)
      }
      phase.nodes.push(node)
    }
    return groups
  })
  let selectedPhase = $derived(phases.find(phase => String(phase.ordinal) === requestedPhase) ?? phases[0] ?? null)
</script>

<Card.Root class="overflow-hidden">
  <div class="grid min-h-[28rem] md:grid-cols-[17rem_1fr]">
    <aside class="space-y-1 border-b bg-muted/20 p-3 md:border-r md:border-b-0" aria-label={$t('workflow.phases')}>
      <div class="px-2 py-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">{$t('workflow.phases')}</div>
      {#each phases as phase (phase.ordinal)}
        <Button variant={selectedPhase?.ordinal === phase.ordinal ? 'secondary' : 'ghost'} class="h-auto w-full justify-start px-2 py-2 text-left" onclick={() => onphase(phase.ordinal)}>
          <span class="w-3 shrink-0"></span>
          <span class="w-5 shrink-0 text-xs text-muted-foreground">{phase.ordinal}</span>
          <span class="min-w-0 flex-1 truncate">{phase.name || $t('workflow.noPhase')}</span>
        </Button>
      {/each}
    </aside>
    <div class="min-w-0 space-y-4 p-4 md:p-6">
      <div class="flex flex-wrap items-center gap-2">
        {#if revision !== null}<h3 class="text-lg font-semibold">{$t('workflow.viewingVersion')}{revision}</h3><Badge variant="secondary">{$t('workflow.historical')}</Badge><Badge variant="outline">{$t('common.readOnly')}</Badge>{/if}
      </div>
      <p class="text-sm text-muted-foreground">{$t('workflow.definitionOnly')}</p>
      {#if revision === null}
        <Alert.Root variant="destructive"><Alert.Title>{$t('workflow.invalidRevision')}</Alert.Title><Alert.Description>{$t('workflow.selectVersion')}</Alert.Description></Alert.Root>
      {:else if revisionQuery.isPending}
        <div role="status" aria-label={$t('workflow.loadingRevision')}><Skeleton class="h-64 w-full" /></div>
      {:else if revisionQuery.error}
        <Alert.Root variant="destructive"><Alert.Title>{$t('workflow.revisionFailed')}</Alert.Title><Alert.Description>{revisionQuery.error.message}</Alert.Description></Alert.Root>
        <Button variant="outline" onclick={() => void revisionQuery.refetch()}>{$t('workflow.retryRevision')}</Button>
      {:else if revisionQuery.data}
        {#if selectedPhase}
          <section>
            <div class="mb-4"><h3 class="text-lg font-semibold">{selectedPhase.name || $t('workflow.noPhase')}</h3><p class="text-sm text-muted-foreground">{selectedPhase.nodes.length} {selectedPhase.nodes.length === 1 ? 'agent' : 'agents'}</p></div>
            <Separator class="mb-2" />
            <div class="divide-y">
            {#each selectedPhase.nodes as node (node.node_id)}
              <div class="space-y-3 px-2 py-4">
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <div><h5 class="font-medium">{node.title}</h5><p class="text-xs text-muted-foreground">{node.node_type} · {node.node_id}</p></div>
                  {#if node.retired_revision !== null}<Badge variant="outline">{$t('workflow.removedVersion')}{node.retired_revision}</Badge>{/if}
                  {#if node.session_id}<Button variant="ghost" onclick={() => node.session_id && navigate(`/chat/${node.session_id}`)}>{$t('chat.openArrow')}</Button>{/if}
                </div>
                <Collapsible.Root>
                  <Collapsible.Trigger class="text-sm font-medium underline">{$t('workflow.definitionDetails')}</Collapsible.Trigger>
                  <Collapsible.Content>
                    <dl class="mt-3 space-y-3 text-sm [&_dd]:whitespace-pre-wrap [&_dd]:break-words [&_dt]:font-medium">
                      <div><dt>{$t('workflow.instructions')}</dt><dd>{node.instructions || $t('common.notProvided')}</dd></div>
                      <div><dt>{$t('workflow.inputs')}</dt><dd>{node.inputs.length ? node.inputs.join('\n') : $t('common.none')}</dd></div>
                      <div><dt>{$t('workflow.output')}</dt><dd>{node.output || $t('common.notProvided')}</dd></div>
                      <div><dt>{$t('profiles.profile')}</dt><dd>{node.execution_profile_id ?? $t('common.notProvided')}{node.execution_profile_version ? ` · ${node.execution_profile_version}` : ''}</dd></div>
                      <div><dt>{$t('workflow.parentNode')}</dt><dd>{node.parent_node_id ?? $t('common.none')}</dd></div>
                      <div><dt>{$t('workflow.introduced')}</dt><dd>v{node.introduced_revision}</dd></div>
                    </dl>
                  </Collapsible.Content>
                </Collapsible.Root>
              </div>
            {/each}
            </div>
          </section>
        {:else}
          <Empty.Root><Empty.Header><Empty.Title>{$t('workflow.noNodes')}</Empty.Title></Empty.Header></Empty.Root>
        {/if}
      {/if}
    </div>
  </div>
</Card.Root>
