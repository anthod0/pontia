<script lang="ts">
  import { onMount } from 'svelte'
  import { navigate } from '$lib/navigation'
  import { CircleAlert, Plus, RefreshCw } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import TaskStateBadge from '../components/tasks/TaskStateBadge.svelte'
  import { formatDateTime, shortId } from '../components/tasks/format'
  import { createDagTask, loadTasks, tasks, tasksError, tasksLoading } from '../stores/tasks'
  import { loadWorkspaces, workspaces, workspacesError, workspacesLoading } from '../stores/workspaces'

  let input = ''
  let workspace = ''
  let clientType = 'pi'
  let creating = false
  let createError: string | null = null

  onMount(() => {
    void Promise.all([loadTasks(), loadWorkspaces()])
  })

  $: sortedTasks = [...$tasks].sort((a, b) => b.updated_at.localeCompare(a.updated_at))
  $: canCreate = input.trim().length > 0 && workspace.trim().length > 0 && !creating

  async function submitDagTask() {
    if (!canCreate) return
    creating = true
    createError = null
    try {
      const result = await createDagTask({ input: input.trim(), workspace, client_type: clientType.trim() || 'pi', metadata: { source: 'dashboard-v2' } })
      input = ''
      navigate(`/tasks/${result.task.task_id}/overview`)
    } catch (error) {
      createError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="text-3xl font-semibold tracking-tight">DAG Tasks</h2>
      <p class="max-w-3xl text-muted-foreground">Create and inspect DAG-managed tasks. Creation uses <code>/external/v1/dag-tasks</code>.</p>
    </div>
    <Button variant="outline" onclick={() => void Promise.all([loadTasks(), loadWorkspaces()])}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  {#if $tasksError || $workspacesError || createError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Task data error</Alert.Title>
      <Alert.Description>{createError ?? $tasksError ?? $workspacesError}</Alert.Description>
    </Alert.Root>
  {/if}

  <Card.Root>
    <Card.Header>
      <Card.Title class="flex items-center gap-2"><Plus class="size-5" /> Create DAG task</Card.Title>
      <Card.Description>Select a registered workspace before creating the task.</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-4">
      <div class="grid gap-4 lg:grid-cols-[1fr_220px_140px]">
        <div class="space-y-2">
          <Label for="task-input">Task request</Label>
          <Textarea id="task-input" bind:value={input} placeholder="Describe the DAG-managed work to plan and execute…" />
        </div>
        <div class="space-y-2">
          <Label for="task-workspace">Workspace</Label>
          <select id="task-workspace" bind:value={workspace} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm" disabled={$workspacesLoading}>
            <option value="">Select workspace…</option>
            {#each $workspaces as item}
              <option value={item.canonical_path}>{item.name ?? item.display_path}</option>
            {/each}
          </select>
        </div>
        <div class="space-y-2">
          <Label for="client-type">Client</Label>
          <Input id="client-type" bind:value={clientType} />
        </div>
      </div>
      <Button disabled={!canCreate} onclick={submitDagTask}>Create DAG task</Button>
      {#if !$workspacesLoading && !$workspaces.length}
        <p class="text-sm text-muted-foreground">No workspaces are registered yet. Register one from the Workspaces page first.</p>
      {/if}
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <Card.Title>Tasks</Card.Title>
      <Card.Description>Latest tasks ordered by update time.</Card.Description>
    </Card.Header>
    <Card.Content>
      {#if $tasksLoading}
        <div class="space-y-2"><Skeleton class="h-10 w-full" /><Skeleton class="h-10 w-full" /><Skeleton class="h-10 w-full" /></div>
      {:else if !sortedTasks.length}
        <Empty.Root>
          <Empty.Header>
            <Empty.Title>No DAG tasks yet</Empty.Title>
            <Empty.Description>Create a DAG task above to start the primary workflow.</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {:else}
        <div class="min-w-0 overflow-x-auto">
          <Table.Root class="table-fixed">
            <Table.Header>
              <Table.Row>
                <Table.Head class="w-[45%]">Task</Table.Head>
                <Table.Head class="w-28">State</Table.Head>
                <Table.Head class="w-[24%]">Workspace</Table.Head>
                <Table.Head class="w-40">Updated</Table.Head>
                <Table.Head class="w-20 text-right">Open</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each sortedTasks as item}
                <Table.Row>
                  <Table.Cell class="max-w-0">
                    <div class="font-medium">{shortId(item.task_id)}</div>
                    <div class="truncate text-sm text-muted-foreground" title={item.input}>{item.input}</div>
                  </Table.Cell>
                  <Table.Cell><TaskStateBadge state={item.state} /></Table.Cell>
                  <Table.Cell class="max-w-0">
                    <div class="truncate" title={item.workspace_id ?? '—'}>{item.workspace_id ?? '—'}</div>
                  </Table.Cell>
                  <Table.Cell>{formatDateTime(item.updated_at)}</Table.Cell>
                  <Table.Cell class="text-right"><Button size="sm" variant="outline" onclick={() => navigate(`/tasks/${item.task_id}/overview`)}>Open</Button></Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</section>
