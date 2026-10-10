<script lang="ts">
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { navigate } from '$lib/navigation'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionStateDotClass } from '$lib/sessionState'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { createSessionOverviewQuery, snapshotSessionOverview } from '../queries/sessionOverview'
  import type { SessionView } from '../api/types'

  let selectedTab = $state('all')
  const overviewQuery = createSessionOverviewQuery(true)
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))
  const activeIds = $derived(new Set(overview.active.map((session) => session.session_id)))
  const listSessions = $derived(overview.list.filter((session) => !activeIds.has(session.session_id)))
  const selectedCount = $derived(
    selectedTab === 'archived'
      ? overview.archived.length
      : selectedTab === 'pinned'
        ? overview.pinned.length
        : overview.active.length + listSessions.length,
  )

  function openSession(sessionId: string): void {
    navigate(`/chat/${sessionId}`)
  }
</script>

{#snippet sessionList(items: SessionView[], testId: string)}
  <div class="divide-y" data-testid={testId}>
    {#each items as session (session.session_id)}
      <button type="button" class="w-full px-1 py-3 text-left transition hover:bg-muted/50 sm:px-2" onclick={() => openSession(session.session_id)}>
        <div class="flex min-w-0 items-start justify-between gap-3">
          <div class="min-w-0 space-y-1">
            <div class="truncate font-medium">{sessionChatTitle(session)}</div>
            <div class="truncate text-sm text-muted-foreground">{session.client_type} · Updated {new Date(session.updated_at).toLocaleString()}</div>
          </div>
          {#if session.state !== 'exited'}
            <Badge variant="secondary" class="shrink-0 gap-1.5">
              <span class={`size-2 rounded-none ${sessionStateDotClass(session.state)}`}></span>
              {session.state}
            </Badge>
          {/if}
        </div>
      </button>
    {/each}
  </div>
{/snippet}

{#snippet emptyState(title: string, description: string)}
  <Card.Root>
    <Card.Content class="py-8">
      <Empty.Root>
        <Empty.Header>
          <Empty.Title>{title}</Empty.Title>
          <Empty.Description>{description}</Empty.Description>
        </Empty.Header>
      </Empty.Root>
    </Card.Content>
  </Card.Root>
{/snippet}

<section class="mx-auto flex w-full max-w-4xl flex-col gap-6" aria-labelledby="sessions-page-title">
  <div class="flex items-center justify-between gap-3">
    <div class="space-y-2">
      <h1 id="sessions-page-title" class="text-3xl font-semibold tracking-tight">Sessions</h1>
      <p class="text-sm text-muted-foreground">Browse active, archived, and pinned sessions.</p>
    </div>
    <Badge variant="secondary">{selectedCount}</Badge>
  </div>

  {#if overviewQuery.error}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>Could not load sessions</Alert.Title>
      <Alert.Description>{overviewQuery.error.message}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if overviewQuery.isPending}
    <Card.Root>
      <Card.Content class="py-6 text-sm text-muted-foreground" role="status">Loading sessions…</Card.Content>
    </Card.Root>
  {:else}
    <Tabs.Root bind:value={selectedTab}>
      <Tabs.List variant="line" aria-label="Session views">
        <Tabs.Trigger value="all">All</Tabs.Trigger>
        <Tabs.Trigger value="archived">Archived</Tabs.Trigger>
        <Tabs.Trigger value="pinned">Pinned</Tabs.Trigger>
      </Tabs.List>

      <Tabs.Content value="all" class="space-y-6 pt-4">
        {#if overview.active.length || listSessions.length}
          {#if overview.active.length}
            <section class="space-y-2" aria-labelledby="active-sessions-title">
              <h2 id="active-sessions-title" class="text-sm font-semibold">Active</h2>
              {@render sessionList(overview.active, 'active-session-list')}
            </section>
          {/if}

          <section class="space-y-2" aria-labelledby="session-list-title">
            <h2 id="session-list-title" class="text-sm font-semibold">All sessions</h2>
            {#if listSessions.length}
              {@render sessionList(listSessions, 'all-session-list')}
            {:else}
              <p class="py-4 text-sm text-muted-foreground">No other sessions.</p>
            {/if}
            {#if overview.nextCursor}
              <div class="flex justify-center pt-2">
                <Button variant="outline" disabled={overviewQuery.isFetchingNextPage} onclick={() => void overviewQuery.fetchNextPage()}>
                  {overviewQuery.isFetchingNextPage ? 'Loading…' : 'Load more'}
                </Button>
              </div>
            {/if}
          </section>
        {:else if !overviewQuery.error}
          {@render emptyState('No sessions', 'Start a new chat to create a session.')}
        {/if}
      </Tabs.Content>

      <Tabs.Content value="archived" class="pt-4">
        {#if overview.archived.length}
          {@render sessionList(overview.archived, 'archived-session-list')}
        {:else}
          {@render emptyState('No archived sessions', 'Archived sessions will appear here.')}
        {/if}
      </Tabs.Content>

      <Tabs.Content value="pinned" class="pt-4">
        {#if overview.pinned.length}
          {@render sessionList(overview.pinned, 'pinned-session-list')}
        {:else}
          {@render emptyState('No pinned sessions', 'Pinned sessions will appear here.')}
        {/if}
      </Tabs.Content>
    </Tabs.Root>
  {/if}
</section>
