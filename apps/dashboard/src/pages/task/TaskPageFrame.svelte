<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import { getPathParams, navigate } from 'svelte-mini-router'
  import { CircleAlert, RefreshCw } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import TaskStateBadge from '../../components/tasks/TaskStateBadge.svelte'
  import { formatDateTime } from '../../components/tasks/format'
  import { refreshTask, selectedTaskId, task, taskError, taskLoading } from '../../stores/tasks'

  let { title, description, children }: { title: string; description: string; children?: Snippet } = $props()

  const { taskId = 'unknown' } = getPathParams()
  const tabs = [
    ['Overview', `/tasks/${taskId}/overview`],
    ['DAG', `/tasks/${taskId}/dag`],
    ['Work Items', `/tasks/${taskId}/work-items`],
    ['Sessions', `/tasks/${taskId}/sessions`],
    ['Artifacts', `/tasks/${taskId}/artifacts`],
    ['Activity', `/tasks/${taskId}/activity`],
  ] as const

  onMount(() => {
    selectedTaskId.set(taskId)
    void refreshTask(taskId)
  })
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
    <div class="space-y-2">
      <p class="text-sm font-medium text-muted-foreground">Task {taskId}</p>
      <div class="flex flex-wrap items-center gap-3">
        <h2 class="text-3xl font-semibold tracking-tight">{title}</h2>
        {#if $task}<TaskStateBadge state={$task.state} />{/if}
      </div>
      <p class="max-w-3xl text-muted-foreground">{description}</p>
      {#if $task}<p class="text-xs text-muted-foreground">Updated {formatDateTime($task.updated_at)}</p>{/if}
    </div>
    <Button variant="outline" onclick={() => void refreshTask(taskId)}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  <div class="flex flex-wrap gap-2">
    {#each tabs as [label, path]}
      <Button variant="outline" size="sm" onclick={() => navigate(path)}>{label}</Button>
    {/each}
  </div>

  {#if $taskError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Unable to load task detail</Alert.Title>
      <Alert.Description>{$taskError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if $taskLoading && !$task}
    <Card.Root><Card.Content class="space-y-2 pt-6"><Skeleton class="h-6 w-1/3" /><Skeleton class="h-24 w-full" /></Card.Content></Card.Root>
  {:else if children}
    {@render children()}
  {/if}
</section>
