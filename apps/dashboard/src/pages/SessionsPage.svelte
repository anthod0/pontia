<script lang="ts">
  import { onMount } from 'svelte'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { navigate } from '$lib/navigation'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionStateDotClass } from '$lib/sessionState'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { loadSessions, sessions, sessionsError, sessionsLoading } from '../stores/sessions'

  const allSessions = $derived(
    $sessions
      .filter((session) => !session.archived_at)
      .slice()
      .sort((left, right) => right.updated_at.localeCompare(left.updated_at) || right.created_at.localeCompare(left.created_at)),
  )

  onMount(() => {
    void loadSessions({ includePinned: true, limit: 200 })
  })

  function openSession(sessionId: string): void {
    navigate(`/chat/${sessionId}`)
  }
</script>

<section class="mx-auto flex w-full max-w-4xl flex-col gap-6" aria-labelledby="sessions-page-title">
  <div class="flex items-center justify-between gap-3">
    <div class="space-y-2">
      <h1 id="sessions-page-title" class="text-3xl font-semibold tracking-tight">All Sessions</h1>
      <p class="text-sm text-muted-foreground">Browse active and completed sessions.</p>
    </div>
    <Badge variant="secondary">{allSessions.length}</Badge>
  </div>

  {#if $sessionsError}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>Could not load sessions</Alert.Title>
      <Alert.Description>{$sessionsError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if $sessionsLoading}
    <Card.Root>
      <Card.Content class="py-6 text-sm text-muted-foreground" role="status">Loading sessions…</Card.Content>
    </Card.Root>
  {:else if allSessions.length}
    <div class="divide-y" data-testid="all-session-list">
      {#each allSessions as session (session.session_id)}
        <button type="button" class="w-full px-1 py-3 text-left transition hover:bg-muted/50 sm:px-2" onclick={() => openSession(session.session_id)}>
          <div class="flex min-w-0 items-start justify-between gap-3">
            <div class="min-w-0 space-y-1">
              <div class="truncate font-medium">{sessionChatTitle(session)}</div>
              <div class="truncate text-sm text-muted-foreground">{session.client_type} · Updated {new Date(session.updated_at).toLocaleString()}</div>
            </div>
            <Badge variant="secondary" class="shrink-0 gap-1.5">
              <span class={`size-2 rounded-none ${sessionStateDotClass(session.state)}`}></span>
              {session.state}
            </Badge>
          </div>
        </button>
      {/each}
    </div>
  {:else if !$sessionsError}
    <Card.Root>
      <Card.Content class="py-8">
        <Empty.Root>
          <Empty.Header>
            <Empty.Title>No sessions</Empty.Title>
            <Empty.Description>Start a new chat to create a session.</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      </Card.Content>
    </Card.Root>
  {/if}
</section>
