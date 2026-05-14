<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import TaskActions from '../../components/tasks/TaskActions.svelte'
  import TaskStateBadge from '../../components/tasks/TaskStateBadge.svelte'
  import { formatDateTime } from '../../components/tasks/format'
  import { task, taskDag } from '../../stores/tasks'
  import TaskPageFrame from './TaskPageFrame.svelte'

  $: openSignals = $taskDag?.signals.filter((signal) => signal.state === 'open') ?? []
  $: blockers = $taskDag?.work_items.filter((item) => item.runtime?.blocked_reason || item.runtime?.current_state === 'blocked') ?? []
</script>

<TaskPageFrame title="Task Overview" description="State, open signals, current blockers, and task-level actions.">
  {#if $task}
    <div class="grid gap-4 lg:grid-cols-3">
      <Card.Root class="lg:col-span-2">
        <Card.Header><Card.Title>Request</Card.Title><Card.Description>{formatDateTime($task.created_at)}</Card.Description></Card.Header>
        <Card.Content class="whitespace-pre-wrap text-sm">{$task.input}</Card.Content>
      </Card.Root>
      <Card.Root>
        <Card.Header><Card.Title>State</Card.Title></Card.Header>
        <Card.Content class="space-y-2 text-sm">
          <div class="flex items-center justify-between"><span class="text-muted-foreground">Task</span><TaskStateBadge state={$task.state} /></div>
          <div class="flex justify-between gap-3"><span class="text-muted-foreground">Workspace</span><span class="text-right">{$task.workspace_id ?? '—'}</span></div>
          <div class="flex justify-between gap-3"><span class="text-muted-foreground">Session</span><span class="text-right">{$task.session_id ?? '—'}</span></div>
          <div class="flex justify-between gap-3"><span class="text-muted-foreground">Routing</span><span class="text-right">{$task.routing_state}</span></div>
        </Card.Content>
      </Card.Root>
    </div>

    <Card.Root>
      <Card.Header><Card.Title>Actions</Card.Title><Card.Description>Supported task-level External API actions.</Card.Description></Card.Header>
      <Card.Content><TaskActions task={$task} /></Card.Content>
    </Card.Root>

    <div class="grid gap-4 lg:grid-cols-2">
      <Card.Root>
        <Card.Header><Card.Title>Open signals</Card.Title><Card.Description>{openSignals.length} need attention</Card.Description></Card.Header>
        <Card.Content>
          {#if openSignals.length}
            <div class="space-y-3">
              {#each openSignals as signal}
                <div class="rounded-lg border p-3 text-sm">
                  <div class="font-medium">{signal.summary}</div>
                  <div class="text-muted-foreground">{signal.kind} · {signal.severity} · {formatDateTime(signal.created_at)}</div>
                  {#if signal.detail}<p class="mt-2 whitespace-pre-wrap">{signal.detail}</p>{/if}
                </div>
              {/each}
            </div>
          {:else}
            <Empty.Root><Empty.Header><Empty.Title>No open signals</Empty.Title><Empty.Description>The DAG has no open signals.</Empty.Description></Empty.Header></Empty.Root>
          {/if}
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header><Card.Title>Current blockers</Card.Title><Card.Description>{blockers.length} blocked work items</Card.Description></Card.Header>
        <Card.Content>
          {#if blockers.length}
            <div class="space-y-3">
              {#each blockers as item}
                <div class="rounded-lg border p-3 text-sm">
                  <div class="font-medium">{item.title}</div>
                  <div class="text-muted-foreground">{item.runtime?.blocked_reason ?? item.runtime?.current_state}</div>
                </div>
              {/each}
            </div>
          {:else}
            <Empty.Root><Empty.Header><Empty.Title>No blockers</Empty.Title><Empty.Description>No blocked work items are reported.</Empty.Description></Empty.Header></Empty.Root>
          {/if}
        </Card.Content>
      </Card.Root>
    </div>
  {/if}
</TaskPageFrame>
