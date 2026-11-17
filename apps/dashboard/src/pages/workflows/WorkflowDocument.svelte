<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  const device = useDeviceRuntime();
  const { createWorkflowDocumentQuery } = device.workflowQueries;

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
      <p class="break-all font-mono text-xs text-muted-foreground">{$t('workflow.documentRef')} {documentRef}</p>
      {#if documentQuery.isPending}<p role="status">{$t('workflow.loadingDocument')}</p>
      {:else if documentQuery.error}<p role="alert" class="text-sm text-destructive">{$t('workflow.cannotRead')} {label}: {documentQuery.error.message}</p><Button variant="outline" size="sm" onclick={() => void documentQuery.refetch()}>{$t('workflow.retryDocument')}</Button>
      {:else if documentQuery.data}<pre class="max-h-96 overflow-auto rounded-md bg-muted p-3 text-sm whitespace-pre-wrap break-words">{documentQuery.data.content || $t('workflow.emptyDocument')}</pre>{/if}
    </Collapsible.Content>
  </Collapsible.Root>
{:else}<p class="text-sm text-muted-foreground">{label}{$t('workflow.notProvidedSuffix')}</p>{/if}
