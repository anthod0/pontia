<script lang="ts">
  import { onMount } from 'svelte'
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
  import {
    loadMoreSessionsPageSessions,
    loadSessionsPageOverview,
    sessionsPageActiveSessions,
    sessionsPageArchivedSessions,
    sessionsPageError,
    sessionsPageListSessions,
    sessionsPageLoading,
    sessionsPageLoadingMore,
    sessionsPageNextCursor,
    sessionsPagePinnedSessions,
  } from '../stores/sessions'
  import type { SessionView } from '../api/types'

  let selectedTab = $state('all')
  const activeIds = $derived(new Set($sessionsPageActiveSessions.map((session) => session.session_id)))
  const listSessions = $derived($sessionsPageListSessions.filter((session) => !activeIds.has(session.session_id)))
  const selectedCount = $derived(
    selectedTab === 'archived'
      ? $sessionsPageArchivedSessions.length
      : selectedTab === 'pinned'
        ? $sessionsPagePinnedSessions.length
        : $sessionsPageActiveSessions.length + listSessions.length,
  )

  onMount(() => {
    void loadSessionsPageOverview()
  })

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

  {#if $sessionsPageError}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>Could not load sessions</Alert.Title>
      <Alert.Description>{$sessionsPageError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if $sessionsPageLoading}
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
        {#if $sessionsPageActiveSessions.length || listSessions.length}
          {#if $sessionsPageActiveSessions.length}
            <section class="space-y-2" aria-labelledby="active-sessions-title">
              <h2 id="active-sessions-title" class="text-sm font-semibold">Active</h2>
              {@render sessionList($sessionsPageActiveSessions, 'active-session-list')}
            </section>
          {/if}

          <section class="space-y-2" aria-labelledby="session-list-title">
            <h2 id="session-list-title" class="text-sm font-semibold">All sessions</h2>
            {#if listSessions.length}
              {@render sessionList(listSessions, 'all-session-list')}
            {:else}
              <p class="py-4 text-sm text-muted-foreground">No other sessions.</p>
            {/if}
            {#if $sessionsPageNextCursor}
              <div class="flex justify-center pt-2">
                <Button variant="outline" disabled={$sessionsPageLoadingMore} onclick={() => void loadMoreSessionsPageSessions()}>
                  {$sessionsPageLoadingMore ? 'Loading…' : 'Load more'}
                </Button>
              </div>
            {/if}
          </section>
        {:else if !$sessionsPageError}
          {@render emptyState('No sessions', 'Start a new chat to create a session.')}
        {/if}
      </Tabs.Content>

      <Tabs.Content value="archived" class="pt-4">
        {#if $sessionsPageArchivedSessions.length}
          {@render sessionList($sessionsPageArchivedSessions, 'archived-session-list')}
        {:else}
          {@render emptyState('No archived sessions', 'Archived sessions will appear here.')}
        {/if}
      </Tabs.Content>

      <Tabs.Content value="pinned" class="pt-4">
        {#if $sessionsPagePinnedSessions.length}
          {@render sessionList($sessionsPagePinnedSessions, 'pinned-session-list')}
        {:else}
          {@render emptyState('No pinned sessions', 'Pinned sessions will appear here.')}
        {/if}
      </Tabs.Content>
    </Tabs.Root>
  {/if}
</section>
