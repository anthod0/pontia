<script lang="ts">
  import { untrack } from 'svelte'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { getWorkflowDocument } from '../../api/client'

  let { workflowId, documentRef, label }: { workflowId: string; documentRef: string | null; label: string } = $props()
  let open = $state(false)
  let content = $state<string | null>(null)
  let error = $state<string | null>(null)
  let loading = $state(false)
  let retry = $state(0)
  $effect(() => {
    const ref = documentRef
    const id = workflowId
    const expanded = open
    retry
    const request = new AbortController()
    untrack(() => { content = null; error = null; loading = expanded && !!ref })
    if (expanded && ref) {
      void getWorkflowDocument(id, ref, { signal: request.signal }).then((document) => {
        if (!request.signal.aborted) { content = document.content; loading = false }
      }).catch((cause: unknown) => {
        if (!request.signal.aborted) { error = cause instanceof Error ? cause.message : String(cause); loading = false }
      })
    }
    return () => request.abort()
  })
</script>

{#if documentRef}
  <Collapsible.Root bind:open>
    <Collapsible.Trigger class="text-sm font-medium underline">{label}</Collapsible.Trigger>
    <Collapsible.Content class="mt-2 space-y-2">
      <p class="break-all font-mono text-xs text-muted-foreground">Document ref: {documentRef}</p>
      {#if loading}<p role="status">Loading document…</p>
      {:else if error}<p role="alert" class="text-sm text-destructive">Could not read {label}: {error}</p><Button variant="outline" size="sm" onclick={() => retry++}>Retry document</Button>
      {:else if content !== null}<pre class="max-h-96 overflow-auto rounded-none bg-muted p-3 text-sm whitespace-pre-wrap break-words">{content || 'Empty document'}</pre>{/if}
    </Collapsible.Content>
  </Collapsible.Root>
{:else}<p class="text-sm text-muted-foreground">{label}: not provided</p>{/if}
