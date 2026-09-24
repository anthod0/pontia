<script lang="ts">
  import { onMount } from 'svelte'
  import { toast } from 'svelte-sonner'
  import ArchiveIcon from 'phosphor-svelte/lib/ArchiveIcon'
  import { navigate } from '$lib/navigation'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { listSessions } from '../api/client'
  import type { SessionView } from '../api/types'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionWorkspacePath, sessionWorkspaceTitle } from '../components/chat/sessionMetadata'
  import { loadSessions, unarchiveSession } from '../stores/sessions'
  import { workspaces } from '../stores/workspaces'

  let archivedSessions = $state<SessionView[]>([])
  let loading = $state(true)
  let loadError = $state<string | null>(null)
  let actionError = $state<string | null>(null)
  let restoringId = $state<string | null>(null)
  let restoredSession = $state<SessionView | null>(null)
  let search = $state('')
  let disposed = false
  let listRequest = 0

  const matchingSessions = $derived(archivedSessions.filter((session) => [
    sessionChatTitle(session), session.client_type, session.session_id,
    sessionWorkspaceTitle(session, $workspaces), sessionWorkspacePath(session, $workspaces),
  ].some((value) => value.toLowerCase().includes(search.trim().toLowerCase()))))

  onMount(() => {
    void refreshArchive()
    return () => { disposed = true; listRequest += 1 }
  })

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error)
  }

  async function refreshArchive(): Promise<void> {
    const request = ++listRequest
    loading = true
    loadError = null
    try {
      const sessions = await listSessions({ includeArchived: true })
      if (request !== listRequest) return
      archivedSessions = sessions.filter((session) => session.archived_at)
        .sort((left, right) => right.archived_at!.localeCompare(left.archived_at!))
    } catch (error) {
      if (request === listRequest) loadError = errorMessage(error)
    } finally {
      if (request === listRequest) loading = false
    }
  }

  async function openRestoredSession(session: SessionView): Promise<void> {
    actionError = null
    try {
      await navigate(`/chat/${encodeURIComponent(session.session_id)}`)
    } catch (error) {
      if (!disposed) actionError = `Session restored, but could not open it: ${errorMessage(error)}`
    }
  }

  async function refreshRestoredSession(session: SessionView): Promise<void> {
    try {
      await loadSessions({ showLoading: false, throwOnError: true })
    } catch (error) {
      if (!disposed) actionError = `Session restored, but the session list could not refresh: ${errorMessage(error)}`
      return
    }
    if (disposed) return
    toast.success('Session restored to the list')
    await openRestoredSession(session)
  }

  async function restore(session: SessionView): Promise<void> {
    if (restoringId || loading) return
    restoringId = session.session_id
    restoredSession = null
    actionError = null
    try {
      const restored = await unarchiveSession(session.session_id)
      if (disposed) return
      restoredSession = restored
      archivedSessions = archivedSessions.filter((item) => item.session_id !== restored.session_id)
      await refreshRestoredSession(restored)
    } catch (error) {
      if (!disposed) actionError = `Restore could not be confirmed for “${sessionChatTitle(session)}”. Refresh or try again. ${errorMessage(error)}`
    } finally {
      restoringId = null
    }
  }

  async function retryListRefresh(): Promise<void> {
    if (!restoredSession || restoringId) return
    restoringId = restoredSession.session_id
    actionError = null
    try {
      await refreshRestoredSession(restoredSession)
    } finally {
      restoringId = null
    }
  }
</script>

<section class="mx-auto flex w-full max-w-4xl flex-col gap-6" aria-label="Archived sessions">
  <div class="flex items-start justify-between gap-3">
    <div class="space-y-2">
      <h1 class="text-3xl font-semibold tracking-tight">Archived sessions</h1>
      <p class="text-sm text-muted-foreground">
        Restore a session to the Pontia list and open its history. Restoring does not start the agent or send a message.
        Exited sessions use the existing resume flow in chat. Codex thread archiving is separate and is not changed here.
      </p>
    </div>
    <Button variant="outline" disabled={loading || restoringId !== null} onclick={() => void refreshArchive()}>Refresh</Button>
  </div>

  <Input aria-label="Search archived sessions" placeholder="Search by title, client, or workspace…" bind:value={search} />

  {#if loadError}
    <Alert.Root variant="destructive">
      <Alert.Title>Could not load archived sessions</Alert.Title>
      <Alert.Description>{loadError} Use Refresh to try again.</Alert.Description>
    </Alert.Root>
  {/if}

  {#if actionError}
    <Alert.Root variant="destructive">
      <Alert.Title>{restoredSession ? 'Session restored; follow-up failed' : 'Could not confirm restore'}</Alert.Title>
      <Alert.Description>{actionError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if restoredSession}
    <div role="status" class="space-y-3 rounded-none border p-4">
      <p class="text-sm">Restored: {sessionChatTitle(restoredSession)}</p>
      <div class="flex flex-wrap gap-2">
        <Button variant="outline" disabled={restoringId !== null} onclick={() => void openRestoredSession(restoredSession!)}>Open session</Button>
        <Button variant="outline" disabled={restoringId !== null} onclick={() => void retryListRefresh()}>Retry list refresh</Button>
      </div>
    </div>
  {/if}

  {#if loading}
    <p role="status" class="text-sm text-muted-foreground">Loading archived sessions…</p>
  {/if}

  {#if matchingSessions.length}
    <ul class="divide-y" aria-label="Archived session list">
      {#each matchingSessions as session (session.session_id)}
        <li class="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between">
          <div class="min-w-0 space-y-1">
            <h2 class="truncate font-medium" title={sessionChatTitle(session)}>{sessionChatTitle(session)}</h2>
            <p class="truncate text-sm text-muted-foreground" title={sessionWorkspacePath(session, $workspaces)}>
              {session.client_type} · {sessionWorkspaceTitle(session, $workspaces)}
            </p>
            <p class="text-xs text-muted-foreground">Archived {new Date(session.archived_at!).toLocaleString()}</p>
            <Badge variant="secondary">Execution: {session.state}</Badge>
          </div>
          <Button
            variant="outline"
            class="shrink-0"
            disabled={loading || restoringId !== null}
            aria-label={`Restore and open ${sessionChatTitle(session)}`}
            onclick={() => void restore(session)}
          >
            {restoringId === session.session_id ? 'Restoring…' : 'Restore and open'}
          </Button>
        </li>
      {/each}
    </ul>
  {:else if !loading && !loadError}
    <Empty.Root>
      <Empty.Header>
        <Empty.Media><ArchiveIcon class="size-6" /></Empty.Media>
        <Empty.Title>{search.trim() ? 'No matching sessions' : 'No archived sessions'}</Empty.Title>
        <Empty.Description>{search.trim() ? 'Try another title, client, or workspace.' : 'Sessions archived from the sidebar will appear here.'}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {/if}
</section>
