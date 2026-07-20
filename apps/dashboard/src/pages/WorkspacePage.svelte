<script lang="ts">
  import { onMount } from 'svelte'
  import { navigate, routeParam } from '$lib/navigation'
  import { CircleAlert, Folder } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import NewChatPanel from '../components/chat/NewChatPanel.svelte'
  import { sessionChatTitle, titleFromInitialPrompt } from '$lib/session-chat/sessionChat'
  import { workspaceTitle } from '../components/chat/sessionMetadata'
  import { rememberOptimisticInitialMessage } from '../stores/optimisticChat'
  import { clearChatDraft } from '../stores/chatDraft'
  import { loadSessionTimeline, resetTimelineState } from '../stores/timeline'
  import { createSession, loadSessionDetail, loadSessions, sessions, sessionsError, sessionsLoading } from '../stores/sessions'
  import { loadWorkspaces, workspaces, workspacesError, workspacesLoading } from '../stores/workspaces'
  import type { SessionView } from '../api/types'

  let { routeWorkspaceId = null }: { routeWorkspaceId?: string | null } = $props()

  const CLIENT_TYPE_OPTIONS = ['pi', 'claude']

  let prompt = $state('')
  let clientType = $state('pi')
  let creating = $state(false)
  let actionError = $state<string | null>(null)

  const workspaceId = $derived(routeWorkspaceId ?? routeParam('workspace') ?? '')
  const selectedWorkspace = $derived($workspaces.find((workspace) => workspace.workspace_id === workspaceId) ?? null)
  const workspaceSessions = $derived(sessionsForWorkspace($sessions, workspaceId))
  const errorMessage = $derived(actionError ?? $workspacesError ?? $sessionsError)
  const canCreate = $derived(Boolean(prompt.trim() && workspaceId && clientType.trim() && !creating))

  onMount(() => {
    void refreshPage()
  })

  function sessionsForWorkspace(items: SessionView[], id: string): SessionView[] {
    return items
      .filter((session) => session.workspace_id === id)
      .slice()
      .sort((left, right) => right.updated_at.localeCompare(left.updated_at) || right.created_at.localeCompare(left.created_at))
  }

  async function refreshPage(): Promise<void> {
    await Promise.all([loadWorkspaces(), loadSessions({ includePinned: true, limit: 200 })])
  }

  async function startWorkspaceChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    try {
      const initialPrompt = prompt.trim()
      const result = await createSession({
        client_type: clientType.trim(),
        workspace_id: workspaceId,
        title: titleFromInitialPrompt(initialPrompt),
        initial_task: { input: initialPrompt, metadata: { source: 'dashboard_workspace' } },
        metadata: { source: 'dashboard_workspace' },
      })
      rememberOptimisticInitialMessage(result.session.session_id, initialPrompt, result.initial_turn)
      clearChatDraft()
      prompt = ''
      resetTimelineState(result.session.session_id)
      navigate(`/chat/${result.session.session_id}`)
      await Promise.all([
        loadSessionDetail(result.session.session_id),
        loadSessionTimeline(result.session.session_id, {
          mode: 'rebuild',
          ...(result.session.capabilities.topology === true ? { topology: true } : {}),
        }),
      ])
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }

  function openSession(sessionId: string): void {
    navigate(`/chat/${sessionId}`)
  }
</script>

<section data-testid="workspace-page" class="mx-auto flex w-full max-w-4xl flex-col gap-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
    <div class="min-w-0 space-y-2">
      {#if selectedWorkspace}
        <div class="flex min-w-0 items-center gap-3">
          <div class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-muted text-muted-foreground">
            <Folder class="size-5" />
          </div>
          <div class="min-w-0">
            <h2 class="truncate text-3xl font-semibold tracking-tight">{workspaceTitle(selectedWorkspace)}</h2>
            <p class="truncate text-sm text-muted-foreground" title={selectedWorkspace.canonical_path}>{selectedWorkspace.canonical_path}</p>
          </div>
        </div>
      {:else if $workspacesLoading}
        <h2 class="text-3xl font-semibold tracking-tight">Loading workspace…</h2>
      {:else}
        <h2 class="text-3xl font-semibold tracking-tight">Workspace not found</h2>
        <p class="text-sm text-muted-foreground">No registered workspace matches {workspaceId}.</p>
      {/if}
    </div>
  </div>

  {#if errorMessage}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Workspace page error</Alert.Title>
      <Alert.Description>{errorMessage}</Alert.Description>
    </Alert.Root>
  {/if}

  <NewChatPanel
    bind:prompt
    workspaceId={workspaceId}
    bind:clientType
    {creating}
    {canCreate}
    workspaces={selectedWorkspace ? [selectedWorkspace] : []}
    workspacesLoading={$workspacesLoading}
    {selectedWorkspace}
    clientTypeOptions={CLIENT_TYPE_OPTIONS}
    fixedWorkspace
    promptDisabled={!selectedWorkspace || creating}
    onStartChat={() => void startWorkspaceChat()}
  />

  <section aria-label="Workspace sessions" class="space-y-3">
    <div class="flex items-center justify-between gap-3">
      <div>
        <h3 class="text-xl font-semibold tracking-tight">Sessions</h3>
      </div>
      <Badge variant="secondary">{workspaceSessions.length}</Badge>
    </div>

    {#if $sessionsLoading}
      <Card.Root><Card.Content class="py-6 text-sm text-muted-foreground">Loading sessions…</Card.Content></Card.Root>
    {:else if workspaceSessions.length}
      <div data-testid="workspace-session-list" class="divide-y">
        {#each workspaceSessions as session (session.session_id)}
          <button type="button" class="w-full px-1 py-3 text-left transition hover:bg-muted/50 sm:px-2" onclick={() => openSession(session.session_id)}>
            <div class="flex min-w-0 items-start justify-between gap-3">
              <div class="min-w-0 space-y-1">
                <div class="truncate font-medium">{sessionChatTitle(session)}</div>
                <div class="truncate text-sm text-muted-foreground">{session.client_type} · Updated {new Date(session.updated_at).toLocaleString()}</div>
              </div>
              <Badge variant="secondary" class="shrink-0">{session.state}</Badge>
            </div>
          </button>
        {/each}
      </div>
    {:else}
      <Card.Root>
        <Card.Content class="py-8">
          <Empty.Root>
            <Empty.Header>
              <Empty.Title>No sessions</Empty.Title>
              <Empty.Description>Create a new session from the prompt above.</Empty.Description>
            </Empty.Header>
          </Empty.Root>
        </Card.Content>
      </Card.Root>
    {/if}
  </section>
</section>
