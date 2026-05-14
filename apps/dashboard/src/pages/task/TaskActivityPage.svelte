<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { formatDateTime, jsonPreview } from '../../components/tasks/format'
  import { taskDag, taskEvents } from '../../stores/tasks'
  import TaskPageFrame from './TaskPageFrame.svelte'

  $: activity = [
    ...$taskEvents.map((event) => ({ at: event.created_at, title: event.event_type, detail: jsonPreview(event.payload), badge: 'event' })),
    ...($taskDag?.signals ?? []).map((signal) => ({ at: signal.created_at, title: signal.summary, detail: `${signal.kind} · ${signal.severity}${signal.detail ? `\n${signal.detail}` : ''}`, badge: `signal:${signal.state}` })),
  ].sort((a, b) => b.at.localeCompare(a.at))
</script>

<TaskPageFrame title="Activity" description="Task events and DAG signals.">
  <Card.Root>
    <Card.Header><Card.Title>Timeline</Card.Title><Card.Description>{activity.length} events and signals</Card.Description></Card.Header>
    <Card.Content>
      {#if !activity.length}
        <Empty.Root><Empty.Header><Empty.Title>No activity</Empty.Title><Empty.Description>No task events or DAG signals are available yet.</Empty.Description></Empty.Header></Empty.Root>
      {:else}
        <div class="space-y-3">
          {#each activity as item}
            <div class="rounded-lg border p-3 text-sm">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="font-medium">{item.title}</div>
                <Badge variant="secondary">{item.badge}</Badge>
              </div>
              <div class="text-xs text-muted-foreground">{formatDateTime(item.at)}</div>
              <pre class="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">{item.detail}</pre>
            </div>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</TaskPageFrame>
