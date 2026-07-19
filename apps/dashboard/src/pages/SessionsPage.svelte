<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, RefreshCw, TerminalSquare } from '@lucide/svelte'
  import { navigate } from '$lib/navigation'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { formatDateTime, shortId } from '../components/tasks/format'
  import type { SessionView } from '../api/types'
  import { isTerminalSession, sessionDisplayTitle, visibleSessionsForFilter, type SessionFilter } from './sessions/sessionList'
  import { loadSessions, sessions, sessionsError, sessionsLoading } from '../stores/sessions'

  let sessionFilter: SessionFilter = 'active'

  onMount(() => {
    void loadSessions()
  })

  $: visibleSessions = visibleSessionsForFilter($sessions, sessionFilter)
  $: activeSessionCount = $sessions.filter((session) => !isTerminalSession(session)).length
  $: exitedSessionCount = $sessions.filter((session) => isTerminalSession(session)).length

  function sessionTitle(session: SessionView): string {
    return sessionDisplayTitle(session)
  }

  function openSession(sessionId: string): void {
    navigate(`/sessions/${sessionId}`)
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><TerminalSquare class="size-7" /> Sessions</h2>
      <p class="max-w-3xl text-muted-foreground">Browse sessions. Select a row to open the dedicated session detail page.</p>
    </div>
    <Button variant="outline" onclick={() => void loadSessions()}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  {#if $sessionsError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Sessions error</Alert.Title>
      <Alert.Description>{$sessionsError}</Alert.Description>
    </Alert.Root>
  {/if}

  <Card.Root>
    <Card.Header>
      <Card.Title>Sessions</Card.Title>
      <Card.Description>{activeSessionCount} active · {exitedSessionCount} exited/error · {$sessions.length} total.</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-4">
      <Tabs.Root bind:value={sessionFilter} class="gap-3">
        <Tabs.List class="grid w-full grid-cols-3 md:w-[28rem]">
          <Tabs.Trigger value="active">Active ({activeSessionCount})</Tabs.Trigger>
          <Tabs.Trigger value="exited">Exited ({exitedSessionCount})</Tabs.Trigger>
          <Tabs.Trigger value="all">All ({$sessions.length})</Tabs.Trigger>
        </Tabs.List>
      </Tabs.Root>

      {#if $sessionsLoading}
        <div class="space-y-2"><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /></div>
      {:else if !$sessions.length}
        <Empty.Root><Empty.Header><Empty.Title>No sessions</Empty.Title><Empty.Description>Open chat to create a session.</Empty.Description></Empty.Header></Empty.Root>
      {:else if !visibleSessions.length}
        <Empty.Root><Empty.Header><Empty.Title>No {sessionFilter} sessions</Empty.Title><Empty.Description>Switch tabs to inspect other session states.</Empty.Description></Empty.Header></Empty.Root>
      {:else}
        <div class="overflow-x-auto">
          <Table.Root>
            <Table.Header>
              <Table.Row>
                <Table.Head>Session</Table.Head>
                <Table.Head>State</Table.Head>
                <Table.Head>Client</Table.Head>
                <Table.Head>Workspace</Table.Head>
                <Table.Head>Profile</Table.Head>
                <Table.Head>Current turn</Table.Head>
                <Table.Head>Updated</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each visibleSessions as session}
                <Table.Row class="cursor-pointer hover:bg-muted/50" onclick={() => openSession(session.session_id)}>
                  <Table.Cell>
                    <button class="text-left font-medium hover:underline" onclick={(event) => { event.stopPropagation(); openSession(session.session_id) }}>
                      {sessionTitle(session)}
                    </button>
                    <div class="text-xs text-muted-foreground">{shortId(session.session_id)}</div>
                  </Table.Cell>
                  <Table.Cell><Badge variant="secondary">{session.state}</Badge></Table.Cell>
                  <Table.Cell>{session.client_type}</Table.Cell>
                  <Table.Cell class="max-w-xs truncate">{session.workspace_id ?? session.workspace ?? '—'}</Table.Cell>
                  <Table.Cell>{session.execution_profile_id ?? '—'}</Table.Cell>
                  <Table.Cell>{session.current_turn_id ? shortId(session.current_turn_id) : '—'}</Table.Cell>
                  <Table.Cell>{formatDateTime(session.updated_at)}</Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</section>
