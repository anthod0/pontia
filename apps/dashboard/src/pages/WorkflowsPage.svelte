<script lang="ts">
  import { statusLabel } from '$lib/i18n/status';
  import { t, currentLocale } from '$lib/i18n';
  import { useDeviceRuntime } from '../device/context';
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import { navigate } from '$lib/navigation'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { formatDateTime, shortId } from '../lib/format'
  const device = useDeviceRuntime();
  const { createWorkflowsQuery } = device.workflowQueries;

  const workflowsQuery = createWorkflowsQuery()
  let workflows = $derived(workflowsQuery.data ?? [])

  function formatElapsed(ms: number): string {
    const seconds = Math.floor(ms / 1000)
    const hours = Math.floor(seconds / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)
    const rest = seconds % 60
    return hours ? $t('chat.hoursMinutes', { hours, minutes }) : minutes ? $t('chat.minutesSeconds', { minutes, seconds: rest }) : $t('chat.seconds', { seconds: rest })
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><TreeStructureIcon class="size-7" /> {$t('workflows.title')}</h2>
    </div>
  </div>

  {#if workflowsQuery.error}
    <Alert.Root variant="destructive"><WarningCircleIcon class="size-4" /><Alert.Title>{$t('workflows.error')}</Alert.Title><Alert.Description>{workflowsQuery.error.message}</Alert.Description></Alert.Root>
  {/if}

  <Card.Root class="bg-transparent ring-0">
    <Card.Header><Card.Title>{$t('workflows.runs')}</Card.Title><Card.Description>{workflows.length} {$t('workflows.recentSuffix')}</Card.Description></Card.Header>
    <Card.Content>
      {#if workflowsQuery.isPending}
        <div class="space-y-2"><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /></div>
      {:else if !workflows.length}
        <Empty.Root><Empty.Header><Empty.Title>{$t('workflows.empty')}</Empty.Title><Empty.Description>{$t('workflows.startHint')}</Empty.Description></Empty.Header></Empty.Root>
      {:else}
        <div class="overflow-x-auto">
          <Table.Root>
            <Table.Header><Table.Row><Table.Head>{$t('workflow.title')}</Table.Head><Table.Head>{$t('common.state')}</Table.Head><Table.Head>{$t('workflow.currentPhase')}</Table.Head><Table.Head>{$t('workflow.agents')}</Table.Head><Table.Head>{$t('common.elapsed')}</Table.Head><Table.Head>{$t('common.created')}</Table.Head></Table.Row></Table.Header>
            <Table.Body>
              {#each workflows as workflow (workflow.workflow_id)}
                <Table.Row class={workflow.observation_error ? 'text-muted-foreground' : 'cursor-pointer hover:bg-muted/50'} onclick={() => !workflow.observation_error && navigate(`/workflows/${workflow.workflow_id}`)}>
                  <Table.Cell><div class="font-medium">{workflow.title}</div><div class="text-xs text-muted-foreground">{shortId(workflow.workflow_id)}</div></Table.Cell>
                  <Table.Cell><Badge variant={workflow.state === 'failed' ? 'destructive' : 'secondary'}>{statusLabel(workflow.state, $t)}</Badge></Table.Cell>
                  <Table.Cell>{workflow.observation_error ? $t('workflow.invalidDefinition') : workflow.current_phase_name ?? '—'}</Table.Cell>
                  <Table.Cell>{workflow.agent_submitted_count}/{workflow.agent_total_count}</Table.Cell>
                  <Table.Cell>{formatElapsed(workflow.elapsed_ms)}</Table.Cell>
                  <Table.Cell>{formatDateTime(workflow.created_at, $currentLocale)}</Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</section>
