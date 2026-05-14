<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, RefreshCw } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { formatDateTime, shortId } from '../../components/tasks/format'
  import { sessionDisplayTitle } from '../sessions/sessionList'
  import { task, taskDag } from '../../stores/tasks'
  import { loadTaskSessions, taskSessions, taskSessionsError, taskSessionsLoading } from '../../stores/sessions'
  import TaskPageFrame from './TaskPageFrame.svelte'

  let selectedSessionId = ''

  onMount(() => {
    void loadTaskSessions($task, $taskDag)
  })

  $: if ($task || $taskDag) void loadTaskSessions($task, $taskDag)
  $: selectedDetail = $taskSessions.find((detail) => detail.session.session_id === selectedSessionId) ?? $taskSessions[0] ?? null
</script>

<TaskPageFrame title="Sessions" description="Advanced execution detail for task-associated sessions. Sessions are not a top-level workflow in dashboard v2.">
  <div class="space-y-4">
    <div class="flex justify-end">
      <Button variant="outline" onclick={() => void loadTaskSessions($task, $taskDag)}><RefreshCw class="size-4" /> Refresh sessions</Button>
    </div>

    {#if $taskSessionsError}
      <Alert.Root variant="destructive">
        <CircleAlert class="size-4" />
        <Alert.Title>Unable to load session diagnostics</Alert.Title>
        <Alert.Description>{$taskSessionsError}</Alert.Description>
      </Alert.Root>
    {/if}

    {#if $taskSessionsLoading}
      <div class="grid gap-4 lg:grid-cols-[20rem_1fr]"><Skeleton class="h-80 w-full" /><Skeleton class="h-80 w-full" /></div>
    {:else if !$taskSessions.length}
      <Empty.Root>
        <Empty.Header>
          <Empty.Title>No associated sessions</Empty.Title>
          <Empty.Description>This task and its DAG runs do not reference a session yet.</Empty.Description>
        </Empty.Header>
      </Empty.Root>
    {:else}
      <div class="grid gap-4 lg:grid-cols-[20rem_minmax(0,1fr)]">
        <Card.Root>
          <Card.Header><Card.Title>Task sessions</Card.Title><Card.Description>{$taskSessions.length} referenced by task, runs, work items, or signals.</Card.Description></Card.Header>
          <Card.Content class="space-y-2">
            {#each $taskSessions as detail}
              <button class="w-full rounded-lg border p-3 text-left text-sm transition hover:bg-muted {selectedDetail?.session.session_id === detail.session.session_id ? 'border-primary bg-muted' : ''}" onclick={() => selectedSessionId = detail.session.session_id}>
                <div class="flex items-center justify-between gap-2"><span class="font-medium">{sessionDisplayTitle(detail.session)}</span><Badge variant="secondary">{detail.session.state}</Badge></div>
                <div class="mt-1 truncate text-xs text-muted-foreground">{detail.session.client_type}</div>
                <div class="mt-2 text-xs text-muted-foreground">Updated {formatDateTime(detail.session.updated_at)}</div>
              </button>
            {/each}
          </Card.Content>
        </Card.Root>

        {#if selectedDetail}
          <div class="space-y-4">
            <Card.Root>
              <Card.Header><Card.Title>{sessionDisplayTitle(selectedDetail.session)}</Card.Title><Card.Description>{selectedDetail.session.description ?? 'No session description.'}</Card.Description></Card.Header>
              <Card.Content class="grid gap-3 text-sm md:grid-cols-2 xl:grid-cols-3">
                {#each [
                  ['Session ID', selectedDetail.session.session_id],
                  ['Client', selectedDetail.session.client_type],
                  ['State', selectedDetail.session.state],
                  ['Workspace', selectedDetail.session.workspace_id ?? selectedDetail.session.workspace ?? '—'],
                  ['Profile', selectedDetail.session.execution_profile_id ?? '—'],
                  ['Current turn', selectedDetail.session.current_turn_id ?? '—'],
                ] as [label, value]}
                  <div class="rounded-lg border p-3"><div class="text-xs uppercase tracking-wide text-muted-foreground">{label}</div><div class="mt-1 break-words font-medium">{value}</div></div>
                {/each}
              </Card.Content>
            </Card.Root>

            <Card.Root>
              <Card.Header><Card.Title>References</Card.Title><Card.Description>Why this session is associated with the task.</Card.Description></Card.Header>
              <Card.Content class="flex flex-wrap gap-2">
                {#each selectedDetail.referencedBy as ref}<Badge variant="secondary">{ref}</Badge>{/each}
              </Card.Content>
            </Card.Root>

            <Card.Root>
              <Card.Header><Card.Title>Turns</Card.Title><Card.Description>{selectedDetail.turns.length} recorded turns.</Card.Description></Card.Header>
              <Card.Content>
                {#if selectedDetail.turns.length}
                  <div class="overflow-x-auto">
                    <Table.Root>
                      <Table.Header><Table.Row><Table.Head>Turn</Table.Head><Table.Head>State</Table.Head><Table.Head>Input</Table.Head><Table.Head>Output</Table.Head><Table.Head>Completed</Table.Head></Table.Row></Table.Header>
                      <Table.Body>
                        {#each selectedDetail.turns as turn}
                          <Table.Row>
                            <Table.Cell class="font-medium">{shortId(turn.turn_id)}</Table.Cell>
                            <Table.Cell><Badge variant="secondary">{turn.state}</Badge></Table.Cell>
                            <Table.Cell class="max-w-xs truncate">{turn.input?.summary ?? '—'}</Table.Cell>
                            <Table.Cell class="max-w-xs truncate">{turn.output?.summary ?? (turn.failure ? 'Failed' : '—')}</Table.Cell>
                            <Table.Cell>{turn.completed_at ? formatDateTime(turn.completed_at) : '—'}</Table.Cell>
                          </Table.Row>
                        {/each}
                      </Table.Body>
                    </Table.Root>
                  </div>
                {:else}
                  <Empty.Root><Empty.Header><Empty.Title>No turns</Empty.Title><Empty.Description>This session has no turn history yet.</Empty.Description></Empty.Header></Empty.Root>
                {/if}
              </Card.Content>
            </Card.Root>

            <Card.Root>
              <Card.Header><Card.Title>Session events</Card.Title><Card.Description>Recent events for diagnostics.</Card.Description></Card.Header>
              <Card.Content>
                {#if selectedDetail.events.length}
                  <div class="space-y-3">
                    {#each selectedDetail.events.slice(0, 25) as event}
                      <div class="rounded-lg border p-3 text-sm">
                        <div class="flex flex-wrap items-center justify-between gap-2"><span class="font-medium">{event.type}</span><span class="text-xs text-muted-foreground">{formatDateTime(event.time)}</span></div>
                        <pre class="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">{JSON.stringify(event.payload, null, 2)}</pre>
                      </div>
                    {/each}
                  </div>
                {:else}
                  <Empty.Root><Empty.Header><Empty.Title>No events</Empty.Title><Empty.Description>No session events are available.</Empty.Description></Empty.Header></Empty.Root>
                {/if}
              </Card.Content>
            </Card.Root>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</TaskPageFrame>
