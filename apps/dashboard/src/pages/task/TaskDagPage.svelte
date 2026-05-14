<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import DagSummaryCards from '../../components/dag/DagSummaryCards.svelte'
  import TaskStateBadge from '../../components/tasks/TaskStateBadge.svelte'
  import { shortId } from '../../components/tasks/format'
  import { taskDag } from '../../stores/tasks'
  import TaskPageFrame from './TaskPageFrame.svelte'
</script>

<TaskPageFrame title="Task DAG" description="Work item graph/list view backed by TaskDagView data.">
  {#if $taskDag}
    <DagSummaryCards dag={$taskDag} />
    <Card.Root>
      <Card.Header><Card.Title>Work item graph</Card.Title><Card.Description>Table/tree v1 with dependency IDs.</Card.Description></Card.Header>
      <Card.Content>
        {#if !$taskDag.work_items.length}
          <Empty.Root><Empty.Header><Empty.Title>No work items yet</Empty.Title><Empty.Description>The planner has not submitted a DAG.</Empty.Description></Empty.Header></Empty.Root>
        {:else}
          <div class="overflow-x-auto">
            <Table.Root>
              <Table.Header><Table.Row><Table.Head>Work item</Table.Head><Table.Head>Kind</Table.Head><Table.Head>State</Table.Head><Table.Head>Depends on</Table.Head><Table.Head>Priority</Table.Head></Table.Row></Table.Header>
              <Table.Body>
                {#each $taskDag.work_items as item}
                  {@const parents = $taskDag.edges.filter((edge) => edge.to_work_item_id === item.work_item_id).map((edge) => shortId(edge.from_work_item_id))}
                  <Table.Row>
                    <Table.Cell><div class="font-medium">{item.title}</div><div class="text-xs text-muted-foreground">{shortId(item.work_item_id)}</div><div class="max-w-xl truncate text-sm text-muted-foreground">{item.description}</div></Table.Cell>
                    <Table.Cell>{item.kind}</Table.Cell>
                    <Table.Cell><TaskStateBadge state={item.runtime?.current_state ?? (item.active ? 'active' : 'inactive')} /></Table.Cell>
                    <Table.Cell>{parents.length ? parents.join(', ') : '—'}</Table.Cell>
                    <Table.Cell>{item.priority}</Table.Cell>
                  </Table.Row>
                {/each}
              </Table.Body>
            </Table.Root>
          </div>
        {/if}
      </Card.Content>
    </Card.Root>
  {/if}
</TaskPageFrame>
