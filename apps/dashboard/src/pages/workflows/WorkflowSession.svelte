<script lang="ts">
  import { untrack } from 'svelte'
  import type { WorkflowDetailView } from '../../api/types'
  import { createSessionQuery } from '../../queries/sessions'
  import { Button } from '$lib/components/ui/button/index.js'
  import { navigate } from '$lib/navigation'

  let { sessionId, snapshot }: { sessionId: string; snapshot: WorkflowDetailView } = $props()
  const sessionQuery = createSessionQuery(() => sessionId, () => false)
  $effect(() => { snapshot; sessionId; untrack(() => void sessionQuery.refetch()) })
</script>

<div class="flex flex-wrap items-center gap-2 text-sm">
  <span class="break-all font-mono text-xs">{sessionId}</span>
  {#if sessionQuery.error}
    <span class="text-destructive">Session unavailable: {sessionQuery.error.message}</span>
    <Button variant="outline" size="sm" onclick={() => void sessionQuery.refetch()}>Retry Session</Button>
  {:else if sessionQuery.data}
    <span>Session now: {sessionQuery.data.state}</span>
  {:else}<span class="text-muted-foreground">Loading Session status…</span>{/if}
  <Button variant="outline" size="sm" onclick={() => navigate(`/chat/${encodeURIComponent(sessionId)}`)}>Open chat</Button>
</div>
