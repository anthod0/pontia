<script lang="ts">
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { createWorkflowDocumentQuery } from '../../queries/workflows'

  let { workflowId, documentRef, label }: { workflowId: string; documentRef: string | null; label: string } = $props()
  let open = $state(false)
  const documentQuery = createWorkflowDocumentQuery(
    () => workflowId,
    () => documentRef,
    () => open,
  )
</script>

{#if documentRef}
  <Collapsible.Root bind:open>
    <Collapsible.Trigger class="text-sm font-medium underline">{label}</Collapsible.Trigger>
    <Collapsible.Content class="mt-2 space-y-2">
      <p class="break-all font-mono text-xs text-muted-foreground">Document ref: {documentRef}</p>
      {#if documentQuery.isPending}<p role="status">Loading document…</p>
      {:else if documentQuery.error}<p role="alert" class="text-sm text-destructive">Could not read {label}: {documentQuery.error.message}</p><Button variant="outline" size="sm" onclick={() => void documentQuery.refetch()}>Retry document</Button>
      {:else if documentQuery.data}<pre class="max-h-96 overflow-auto rounded-none bg-muted p-3 text-sm whitespace-pre-wrap break-words">{documentQuery.data.content || 'Empty document'}</pre>{/if}
    </Collapsible.Content>
  </Collapsible.Root>
{:else}<p class="text-sm text-muted-foreground">{label}: not provided</p>{/if}
