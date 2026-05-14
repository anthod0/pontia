<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js'
  import type { TaskDagView } from '../../api/types'

  let { dag }: { dag: TaskDagView } = $props()

  const items = $derived([
    ['Work items', dag.summary.total_work_items],
    ['Ready', dag.summary.ready_work_items],
    ['Running', dag.summary.running_work_items],
    ['Completed', dag.summary.completed_work_items],
    ['Blocked', dag.summary.blocked_work_items],
    ['Failed', dag.summary.failed_work_items],
    ['Open signals', dag.summary.open_signals],
    ['Runs', dag.summary.total_runs],
  ] as const)
</script>

<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
  {#each items as [label, value]}
    <Card.Root>
      <Card.Header class="pb-2">
        <Card.Description>{label}</Card.Description>
      </Card.Header>
      <Card.Content class="text-2xl font-semibold">{value}</Card.Content>
    </Card.Root>
  {/each}
</div>
