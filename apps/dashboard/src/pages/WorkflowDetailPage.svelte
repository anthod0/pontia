<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, Pause, Play, RefreshCw, Workflow } from '@lucide/svelte'
  import { navigate } from '$lib/navigation'
  import { cn } from '$lib/utils.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { Separator } from '$lib/components/ui/separator/index.js'
  import { workflowDetail, workflowDetailError, workflowDetailLoading, pauseWorkflow, refreshWorkflow, resumeWorkflow, selectedWorkflowId } from '../stores/workflows'
  import { groupWorkflowPhases, selectedPhaseOrdinal } from './workflows/phases'
  import type { WorkflowAgentStatus, WorkflowDetailView } from '../api/types'

  let { routeWorkflowId }: { routeWorkflowId: string } = $props()
  let requestedPhase = $state(new URLSearchParams(window.location.search).get('phase'))
  let snapshot = $derived($workflowDetail?.workflow_id === routeWorkflowId ? $workflowDetail : null)
  let phases = $derived(groupWorkflowPhases(snapshot?.nodes ?? [], snapshot?.current_node_id ?? null))
  let explicitOrdinal = $derived(selectedPhaseOrdinal(requestedPhase, phases))
  let selectedPhase = $derived(phases.find((phase) => phase.ordinal === explicitOrdinal) ?? phases.find((phase) => phase.current) ?? phases[0] ?? null)
  let pollTimer: ReturnType<typeof setInterval> | null = null
  let actionBusy = $state(false)

  function readPhaseQuery(): void {
    requestedPhase = new URLSearchParams(window.location.search).get('phase')
  }

  function syncPolling(detail: WorkflowDetailView | null): void {
    const shouldPoll = detail?.workflow_id === routeWorkflowId && ['running', 'paused'].includes(detail.state) && document.visibilityState === 'visible'
    if (shouldPoll && !pollTimer) {
      pollTimer = setInterval(() => void refreshWorkflow(routeWorkflowId, { showLoading: false }), 2000)
    } else if (!shouldPoll && pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  function handleVisibility(): void {
    syncPolling($workflowDetail)
    if (document.visibilityState === 'visible' && snapshot && ['running', 'paused'].includes(snapshot.state)) void refreshWorkflow(routeWorkflowId, { showLoading: false })
  }

  onMount(() => {
    selectedWorkflowId.set(routeWorkflowId)
    const unsubscribe = workflowDetail.subscribe(syncPolling)
    document.addEventListener('visibilitychange', handleVisibility)
    void refreshWorkflow(routeWorkflowId)
    return () => {
      unsubscribe()
      document.removeEventListener('visibilitychange', handleVisibility)
      if (pollTimer) clearInterval(pollTimer)
      selectedWorkflowId.set(null)
    }
  })

  function formatElapsed(ms: number): string {
    const seconds = Math.floor(ms / 1000)
    const hours = Math.floor(seconds / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)
    const rest = seconds % 60
    return hours ? `${hours}h ${minutes}m ${rest}s` : `${minutes}m ${rest}s`
  }

  async function runControl(action: 'pause' | 'resume'): Promise<void> {
    if (actionBusy) return
    actionBusy = true
    try {
      if (action === 'pause') await pauseWorkflow(routeWorkflowId)
      else await resumeWorkflow(routeWorkflowId)
    } catch (error) {
      await refreshWorkflow(routeWorkflowId, { showLoading: false })
      workflowDetailError.set(error instanceof Error ? error.message : String(error))
    } finally {
      actionBusy = false
    }
  }

  function statusClass(status: WorkflowAgentStatus): string {
    if (status === 'submitted') return 'text-green-600 dark:text-green-400'
    if (status === 'paused') return 'text-blue-600 dark:text-blue-400'
    if (status === 'idle') return 'text-emerald-600 dark:text-emerald-400'
    if (status === 'failed') return 'text-destructive'
    if (status === 'starting' || status === 'running') return 'text-amber-600 dark:text-amber-400'
    return 'text-muted-foreground'
  }

  function statusGlyph(status: WorkflowAgentStatus): string {
    if (status === 'pending') return '○'
    if (status === 'unknown') return '?'
    return '●'
  }
</script>

<svelte:window onpopstate={readPhaseQuery} />

<section class="space-y-6">
  <div class="flex items-start justify-between gap-4">
    <div class="min-w-0 space-y-2">
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><Workflow class="size-7 shrink-0" /> <span class="truncate">{snapshot?.title ?? 'Workflow'}</span></h2>
      {#if snapshot}<div class="flex flex-wrap items-center gap-2 text-sm text-muted-foreground"><Badge variant={snapshot.state === 'failed' ? 'destructive' : 'secondary'}>{snapshot.state}</Badge><span>{snapshot.agent_submitted_count}/{snapshot.agent_total_count} agents</span><span>·</span><span>{formatElapsed(snapshot.elapsed_ms)}</span><span>·</span><span class="font-mono text-xs">{snapshot.workflow_id}</span></div>{/if}
    </div>
    <div class="flex shrink-0 gap-2">
      {#if snapshot?.state === 'running'}
        <Button variant="outline" disabled={actionBusy} onclick={() => void runControl('pause')}><Pause class="size-4" /> Pause</Button>
      {:else if snapshot?.state === 'paused'}
        <Button variant="outline" disabled={actionBusy} onclick={() => void runControl('resume')}><Play class="size-4" /> Resume</Button>
      {/if}
      <Button variant="outline" disabled={actionBusy} onclick={() => void refreshWorkflow(routeWorkflowId)}><RefreshCw class="size-4" /> Refresh</Button>
    </div>
  </div>

  {#if $workflowDetailError}
    <Alert.Root variant="destructive"><CircleAlert class="size-4" /><Alert.Title>Workflow error</Alert.Title><Alert.Description>{$workflowDetailError}</Alert.Description></Alert.Root>
  {/if}
  {#if snapshot?.failure_message}
    <Alert.Root variant="destructive"><CircleAlert class="size-4" /><Alert.Title>Workflow failed</Alert.Title><Alert.Description>{snapshot.failure_message}</Alert.Description></Alert.Root>
  {/if}

  {#if $workflowDetailLoading && !snapshot}
    <div class="space-y-3"><Skeleton class="h-24 w-full" /><Skeleton class="h-80 w-full" /></div>
  {:else if snapshot && selectedPhase}
    <Card.Root class="overflow-hidden">
      <div class="grid min-h-[28rem] md:grid-cols-[17rem_1fr]">
        <aside class="border-b bg-muted/20 p-3 md:border-r md:border-b-0">
          <div class="px-2 py-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">Phases</div>
          <div class="space-y-1">
            {#each phases as phase (phase.ordinal)}
              <Button
                variant={selectedPhase.ordinal === phase.ordinal ? 'secondary' : 'ghost'}
                class="h-auto w-full justify-start px-2 py-2 text-left"
                onclick={() => void navigate(`/workflows/${routeWorkflowId}`, { phase: String(phase.ordinal) })}
              >
                <span class={cn('w-3 shrink-0 font-semibold', phase.current ? 'text-primary' : 'text-transparent')}>›</span>
                <span class="w-5 shrink-0 text-xs text-muted-foreground">{phase.ordinal}</span>
                <span class="min-w-0 flex-1 truncate">{phase.name}</span>
                {#if phase.submittedCount > 0 || phase.current}
                  <span class={cn('text-xs tabular-nums', phase.submittedCount === phase.nodes.length ? 'text-green-600 dark:text-green-400' : 'text-muted-foreground')}>{phase.submittedCount}/{phase.nodes.length}</span>
                {/if}
              </Button>
            {/each}
          </div>
        </aside>

        <div class="p-4 md:p-6">
          <div class="mb-4"><h3 class="text-lg font-semibold">{selectedPhase.name}</h3><p class="text-sm text-muted-foreground">{selectedPhase.nodes.length} {selectedPhase.nodes.length === 1 ? 'agent' : 'agents'}</p></div>
          <Separator class="mb-2" />
          <div class="divide-y">
            {#each selectedPhase.nodes as node (node.node_id)}
              <button
                type="button"
                disabled={!node.session_id}
                class="flex w-full items-center gap-3 rounded-md px-2 py-4 text-left disabled:cursor-default enabled:hover:bg-muted/50"
                onclick={() => node.session_id && navigate(`/chat/${node.session_id}`)}
              >
                <span class={cn('w-4 shrink-0 text-center text-lg leading-none', statusClass(node.status))}>{statusGlyph(node.status)}</span>
                <span class="min-w-0 flex-1"><span class="block truncate font-medium">{node.title}</span><span class="block text-xs text-muted-foreground">{node.status}{node.session_state ? ` · session ${node.session_state}` : ''}</span></span>
                {#if node.session_id}<span class="text-xs text-muted-foreground">Open chat →</span>{/if}
              </button>
            {/each}
          </div>
        </div>
      </div>
    </Card.Root>
  {:else if !$workflowDetailLoading && !$workflowDetailError}
    <Empty.Root><Empty.Header><Empty.Title>Workflow unavailable</Empty.Title><Empty.Description>No observable Workflow snapshot was returned.</Empty.Description></Empty.Header></Empty.Root>
  {/if}
</section>
