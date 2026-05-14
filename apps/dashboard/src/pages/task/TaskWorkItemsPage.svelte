<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import TaskStateBadge from '../../components/tasks/TaskStateBadge.svelte'
  import { formatDateTime, jsonPreview, shortId } from '../../components/tasks/format'
  import { taskDag } from '../../stores/tasks'
  import TaskPageFrame from './TaskPageFrame.svelte'
</script>

<TaskPageFrame title="Work Items" description="Work item runtime state and run summaries.">
  {#if $taskDag}
    <Card.Root>
      <Card.Header><Card.Title>Runtime state</Card.Title><Card.Description>{$taskDag.work_items.length} work items · {$taskDag.runs.length} runs</Card.Description></Card.Header>
      <Card.Content>
        {#if !$taskDag.work_items.length}
          <Empty.Root><Empty.Header><Empty.Title>No work items</Empty.Title><Empty.Description>No DAG work items have been created.</Empty.Description></Empty.Header></Empty.Root>
        {:else}
          <div class="overflow-x-auto">
            <Table.Root>
              <Table.Header><Table.Row><Table.Head>Item</Table.Head><Table.Head>Runtime</Table.Head><Table.Head>Attempt</Table.Head><Table.Head>Run</Table.Head><Table.Head>Updated</Table.Head></Table.Row></Table.Header>
              <Table.Body>
                {#each $taskDag.work_items as item}
                  {@const run = $taskDag.runs.find((candidate) => candidate.run_id === item.runtime?.current_run_id) ?? $taskDag.runs.find((candidate) => candidate.work_item_id === item.work_item_id)}
                  <Table.Row>
                    <Table.Cell><div class="font-medium">{item.title}</div><div class="text-xs text-muted-foreground">{shortId(item.work_item_id)} · {item.action}</div></Table.Cell>
                    <Table.Cell><TaskStateBadge state={item.runtime?.current_state ?? 'not_started'} />{#if item.runtime?.blocked_reason}<div class="text-xs text-destructive">{item.runtime.blocked_reason}</div>{/if}</Table.Cell>
                    <Table.Cell>{item.runtime?.current_attempt ?? 0}</Table.Cell>
                    <Table.Cell>{#if run}<div>{shortId(run.run_id)} · {run.state}</div><div class="max-w-md truncate text-xs text-muted-foreground">{run.output_summary ?? jsonPreview(run.failure)}</div>{:else}—{/if}</Table.Cell>
                    <Table.Cell>{formatDateTime(item.runtime?.updated_at ?? item.updated_at)}</Table.Cell>
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
