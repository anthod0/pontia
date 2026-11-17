<script lang="ts">
  import { statusLabel } from '$lib/i18n/status';
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { untrack } from 'svelte'
  import type { WorkflowDetailView } from '../../api/types'
  import { Button } from '$lib/components/ui/button/index.js'
  import { navigate } from '$lib/navigation'
  const device = useDeviceRuntime();
  const { createSessionQuery } = device.sessionQueries;

  let { sessionId, snapshot }: { sessionId: string; snapshot: WorkflowDetailView } = $props()
  const sessionQuery = createSessionQuery(() => sessionId, () => false)
  $effect(() => { snapshot; sessionId; untrack(() => void sessionQuery.refetch()) })
</script>

<div class="flex flex-wrap items-center gap-2 text-sm">
  <span class="break-all font-mono text-xs">{sessionId}</span>
  {#if sessionQuery.error}
    <span class="text-destructive">{$t('workflow.sessionUnavailable')} {sessionQuery.error.message}</span>
    <Button variant="outline" size="sm" onclick={() => void sessionQuery.refetch()}>{$t('workflow.retrySession')}</Button>
  {:else if sessionQuery.data}
    <span>{$t('workflow.sessionNow')} {statusLabel(sessionQuery.data.state, $t)}</span>
  {:else}<span class="text-muted-foreground">{$t('workflow.loadingSession')}</span>{/if}
  <Button variant="outline" size="sm" onclick={() => navigate(`/chat/${encodeURIComponent(sessionId)}`)}>{$t('chat.open')}</Button>
</div>
