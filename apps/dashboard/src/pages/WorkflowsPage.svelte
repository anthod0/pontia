<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, RefreshCw, Workflow } from '@lucide/svelte'
  import { navigate } from '$lib/navigation'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { formatDateTime, shortId } from '../components/tasks/format'
  import { loadWorkflows, workflows, workflowsError, workflowsLoading } from '../stores/workflows'

  onMount(() => { void loadWorkflows() })

  function formatElapsed(ms: number): string {
    const seconds = Math.floor(ms / 1000)
    const hours = Math.floor(seconds / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)
    const rest = seconds % 60
    return hours ? `${hours}h ${minutes}m` : minutes ? `${minutes}m ${rest}s` : `${rest}s`
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><Workflow class="size-7" /> Workflows</h2>
      <p class="max-w-3xl text-muted-foreground">Observe Workflow progress and open the agent Session for each node.</p>
    </div>
    <Button variant="outline" onclick={() => void loadWorkflows()}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  {#if $workflowsError}
    <Alert.Root variant="destructive"><CircleAlert class="size-4" /><Alert.Title>Workflows error</Alert.Title><Alert.Description>{$workflowsError}</Alert.Description></Alert.Root>
  {/if}

  <Card.Root>
    <Card.Header><Card.Title>Workflow runs</Card.Title><Card.Description>{$workflows.length} recent workflows.</Card.Description></Card.Header>
    <Card.Content>
      {#if $workflowsLoading}
        <div class="space-y-2"><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /></div>
      {:else if !$workflows.length}
        <Empty.Root><Empty.Header><Empty.Title>No workflows</Empty.Title><Empty.Description>Run a Workflow with pontiactl to observe it here.</Empty.Description></Empty.Header></Empty.Root>
      {:else}
        <div class="overflow-x-auto">
          <Table.Root>
            <Table.Header><Table.Row><Table.Head>Workflow</Table.Head><Table.Head>State</Table.Head><Table.Head>Current phase</Table.Head><Table.Head>Agents</Table.Head><Table.Head>Elapsed</Table.Head><Table.Head>Created</Table.Head></Table.Row></Table.Header>
            <Table.Body>
              {#each $workflows as workflow (workflow.workflow_id)}
                <Table.Row class={workflow.observation_error ? 'text-muted-foreground' : 'cursor-pointer hover:bg-muted/50'} onclick={() => !workflow.observation_error && navigate(`/workflows/${workflow.workflow_id}`)}>
                  <Table.Cell><div class="font-medium">{workflow.title}</div><div class="text-xs text-muted-foreground">{shortId(workflow.workflow_id)}</div></Table.Cell>
                  <Table.Cell><Badge variant={workflow.state === 'failed' ? 'destructive' : 'secondary'}>{workflow.state}</Badge></Table.Cell>
                  <Table.Cell>{workflow.observation_error ? 'Invalid definition — re-run' : workflow.current_phase_name ?? '—'}</Table.Cell>
                  <Table.Cell>{workflow.agent_submitted_count}/{workflow.agent_total_count}</Table.Cell>
                  <Table.Cell>{formatElapsed(workflow.elapsed_ms)}</Table.Cell>
                  <Table.Cell>{formatDateTime(workflow.created_at)}</Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</section>
