<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { Activity, GitBranch, LogOut, MoreHorizontal, RotateCw, TerminalSquare } from '@lucide/svelte'
  import { toast } from 'svelte-sonner'
  import { getPathParams, navigate } from 'svelte-mini-router'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import SessionMessageComposer from '$lib/components/session-chat/SessionMessageComposer.svelte'
  import type { AgentProfileView, DagProposalView, DashboardStreamEvent, JsonObject, SessionView, WorkspaceView } from '../api/types'
  import {
    canSendSessionMessage,
    isTerminalChatSession,
    turnsToChatMessages,
  } from '$lib/session-chat/sessionChat'
  import {
    clientTypeOptionsForProfile,
    defaultHandleForProfile,
    loadAgentProfiles,
    sessionProfileFields,
    agentProfiles,
    agentProfilesError,
    agentProfilesLoading,
  } from '../stores/agentProfiles'
  import {
    loadWorkspaces,
    workspaces,
    workspacesError,
    workspacesLoading,
  } from '../stores/workspaces'
  import {
    createSession,
    loadSessionDetail,
    loadSessions,
    restartSession,
    resumeSession,
    sessionDetail,
    sessionDetailError,
    sessionDetailLoading,
    sessions,
    sessionsError,
    submitInboxMessage,
    terminateSession,
  } from '../stores/sessions'
  import {
    createDagTask,
    loadTaskProposals,
    taskProposals,
    taskProposalsError,
    taskProposalsLoading,
  } from '../stores/tasks'
  import { subscribeDashboardEvents } from '../services/eventStream'

  let selectedSessionId = ''
  let prompt = ''
  let createWorkspaceId = ''
  let createProfileId = ''
  let createClientType = 'pi'
  let taskMode = false
  let creating = false

  let input = ''
  let submitting = false
  let actionBusy = false
  let actionError: string | null = null
  let lastToastedError: string | null = null
  let advancedControlsOpen = false
  let loadedProposalTaskId = ''
  let appliedRedirectTaskId = ''
  let unsubscribeDashboardEvents: (() => void) | null = null

  onMount(async () => {
    selectedSessionId = requestedSessionIdFromLocation()
    await Promise.all([loadSessions(), loadWorkspaces(), loadAgentProfiles()])
    if (!createWorkspaceId && $workspaces.length) createWorkspaceId = $workspaces[0].workspace_id
    if (selectedSessionId) await loadSessionDetail(selectedSessionId)
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent)
  })

  onDestroy(() => {
    unsubscribeDashboardEvents?.()
  })

  $: selectedSession = selectedSessionId ? ($sessions.find((session) => session.session_id === selectedSessionId) ?? $sessionDetail?.session ?? null) : null
  $: messages = $sessionDetail && $sessionDetail.session.session_id === selectedSessionId ? turnsToChatMessages($sessionDetail.turns) : []
  $: selectedProfile = $agentProfiles.find((profile) => profile.profile_id === createProfileId) ?? null
  $: selectedWorkspace = $workspaces.find((workspace) => workspace.workspace_id === createWorkspaceId) ?? null
  $: clientTypeOptions = clientTypeOptionsForProfile(selectedProfile)
  $: if (!clientTypeOptions.includes(createClientType)) createClientType = clientTypeOptions[0] ?? createClientType
  $: if (!createWorkspaceId && $workspaces.length) createWorkspaceId = $workspaces[0].workspace_id
  $: canCreate = Boolean(prompt.trim() && createWorkspaceId && createClientType.trim() && !creating)
  $: canSend = canSendSessionMessage(selectedSession, input) && !submitting
  $: plannerTaskId = plannerTaskIdForSession(selectedSession)
  $: plannerTaskProposals = plannerTaskId ? $taskProposals.filter((proposal) => proposal.task_id === plannerTaskId) : []
  $: draftPlannerProposal = plannerTaskProposals.find((proposal) => proposal.mode === 'initial_dag' && proposal.state === 'proposed') ?? null
  $: if (plannerTaskId && plannerTaskId !== loadedProposalTaskId) {
    loadedProposalTaskId = plannerTaskId
    void loadTaskProposals(plannerTaskId)
  }
  $: if (plannerTaskId && plannerTaskProposals.some((proposal) => proposal.state === 'applied')) navigateToTaskDag(plannerTaskId)
  $: errorMessage = actionError ?? $sessionDetailError ?? $sessionsError ?? $workspacesError ?? $agentProfilesError ?? $taskProposalsError
  $: {
    if (errorMessage && errorMessage !== lastToastedError) {
      toast.error('Chat error', { description: errorMessage })
      lastToastedError = errorMessage
    }
    if (!errorMessage) lastToastedError = null
  }

  function requestedSessionIdFromLocation(): string {
    return getPathParams().sessionId ?? new URLSearchParams(window.location.search).get('session') ?? ''
  }

  function workspaceTitle(workspace: WorkspaceView): string {
    return workspace.name ?? workspace.display_path ?? workspace.workspace_id
  }

  function profileTitle(profile: AgentProfileView): string {
    return `${profile.name} (${profile.profile_id}@${profile.version})`
  }

  function clientTitle(clientType: string): string {
    return clientType || 'Client'
  }

  function sessionProfileTitle(session: SessionView): string {
    if (!session.execution_profile_id) return '—'
    return session.execution_profile_version ? `${session.execution_profile_id}@${session.execution_profile_version}` : session.execution_profile_id
  }

  function sessionWorkspacePath(session: SessionView): string {
    return session.workspace ?? session.workspace_id ?? 'No workspace'
  }

  function plannerTaskIdForSession(session: SessionView | null): string | null {
    if (!session?.metadata) return null
    const metadata = session.metadata
    const taskId = typeof metadata.task_id === 'string' ? metadata.task_id : null
    const role = typeof metadata.dag_planning_role === 'string' ? metadata.dag_planning_role : null
    return metadata.dag_managed === true && role === 'planner' && taskId ? taskId : null
  }

  function proposalWorkItems(proposal: DagProposalView | null): JsonObject[] {
    const workItems = proposal?.proposal_json.work_items
    return Array.isArray(workItems) ? workItems.filter(isJsonObject) : []
  }

  function proposalEdges(proposal: DagProposalView | null): JsonObject[] {
    const edges = proposal?.proposal_json.edges
    return Array.isArray(edges) ? edges.filter(isJsonObject) : []
  }

  function isJsonObject(value: unknown): value is JsonObject {
    return Boolean(value && typeof value === 'object' && !Array.isArray(value))
  }

  function stringField(value: JsonObject, key: string, fallback = '—'): string {
    const field = value[key]
    return typeof field === 'string' && field.trim() ? field : fallback
  }

  function navigateToTaskDag(taskId: string): void {
    if (appliedRedirectTaskId === taskId) return
    appliedRedirectTaskId = taskId
    navigate(`/tasks/${taskId}/dag`)
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent): void {
    if (!plannerTaskId || streamEvent.kind !== 'task_event') return
    if (streamEvent.event.task_id === plannerTaskId && streamEvent.event.event_type === 'dag.approved') {
      navigateToTaskDag(plannerTaskId)
    }
  }

  function openSessionConsole(): void {
    navigate(selectedSessionId ? `/sessions/${selectedSessionId}` : '/sessions')
  }

  function openNewChat(): void {
    selectedSessionId = ''
    actionError = null
    navigate('/chat')
  }

  function applyProfileDefaults(): void {
    if (!selectedProfile) return
    createClientType = clientTypeOptionsForProfile(selectedProfile)[0] ?? createClientType
  }

  async function selectSessionFromLocation(): Promise<void> {
    const nextSessionId = requestedSessionIdFromLocation()
    if (nextSessionId === selectedSessionId) return
    selectedSessionId = nextSessionId
    input = ''
    actionError = null
    if (selectedSessionId) await loadSessionDetail(selectedSessionId)
  }

  async function startChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    try {
      if (taskMode) {
        const result = await createDagTask({
          input: prompt.trim(),
          workspace: selectedWorkspace?.canonical_path ?? selectedWorkspace?.display_path ?? createWorkspaceId,
          client_type: createClientType.trim() || 'pi',
          metadata: { source: 'dashboard_chat', action: 'manual_task' },
        })
        selectedSessionId = result.planning_turn.session_id
        prompt = ''
        navigate(`/chat/${result.planning_turn.session_id}`)
        await loadSessionDetail(result.planning_turn.session_id)
        return
      }

      const result = await createSession({
        client_type: createClientType.trim(),
        workspace_id: createWorkspaceId,
        handle: defaultHandleForProfile(selectedProfile) || null,
        role: selectedProfile?.default_session_role ?? null,
        description: selectedProfile?.default_session_description ?? null,
        ...sessionProfileFields(selectedProfile),
        initial_task: { input: prompt.trim(), metadata: { source: 'dashboard_chat' } },
        metadata: { source: 'dashboard_chat' },
      })
      selectedSessionId = result.session.session_id
      prompt = ''
      navigate(`/chat/${result.session.session_id}`)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }

  async function runSessionLifecycle(action: 'exit' | 'resume' | 'restart'): Promise<void> {
    if (!selectedSessionId) return
    actionBusy = true
    actionError = null
    try {
      if (action === 'exit') await terminateSession(selectedSessionId)
      if (action === 'resume') await resumeSession(selectedSessionId)
      if (action === 'restart') await restartSession(selectedSessionId)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }

  async function sendMessage(): Promise<void> {
    if (!canSend || !selectedSessionId) return
    submitting = true
    actionError = null
    const message = input.trim()
    try {
      if (selectedSession?.state === 'exited') await resumeSession(selectedSessionId)
      await submitInboxMessage(selectedSessionId, {
        input: message,
        delivery_policy: 'after_idle',
        metadata: { source: 'dashboard_chat' },
      })
      input = ''
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      submitting = false
    }
  }
</script>

<svelte:window onpopstate={() => void selectSessionFromLocation()} />

<section class="flex h-full min-h-0 flex-col gap-4">
  {#if !selectedSessionId}
    <div class="space-y-2">
      <h2 class="text-3xl font-semibold tracking-tight">New Chat</h2>
      <p class="max-w-3xl text-muted-foreground">Start a new agent session from a prompt, workspace, client, and profile.</p>
    </div>
  {/if}

  {#if !selectedSessionId}
    <div class="flex min-h-0 flex-1 items-center justify-center">
      <PromptInput.Root class="w-full max-w-4xl space-y-3" onSubmit={() => void startChat()}>
        <div class="flex min-w-0 flex-wrap items-center gap-2 px-1">
          <Select.Root type="single" bind:value={createWorkspaceId} disabled={$workspacesLoading}>
            <Select.Trigger class="max-w-56" aria-label="Workspace" title={selectedWorkspace?.canonical_path ?? undefined}>
              {#if selectedWorkspace}{workspaceTitle(selectedWorkspace)}{:else}Workspace{/if}
            </Select.Trigger>
            <Select.Content align="start">
              {#each $workspaces as workspace (workspace.workspace_id)}
                <Select.Item value={workspace.workspace_id} label={workspaceTitle(workspace)}>
                  <div class="flex min-w-0 flex-col">
                    <span class="truncate">{workspaceTitle(workspace)}</span>
                    <span class="truncate text-xs text-muted-foreground">{workspace.display_path}</span>
                  </div>
                </Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>

          <Select.Root type="single" bind:value={createProfileId} disabled={$agentProfilesLoading} onValueChange={applyProfileDefaults}>
            <Select.Trigger class="max-w-56" aria-label="Profile">
              {#if selectedProfile}{profileTitle(selectedProfile)}{:else}Profile{/if}
            </Select.Trigger>
            <Select.Content align="start">
              <Select.Item value="" label="No profile">No profile</Select.Item>
              {#each $agentProfiles as profile (profile.profile_id)}
                <Select.Item value={profile.profile_id} label={profileTitle(profile)}>{profileTitle(profile)}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>

          <Select.Root type="single" bind:value={createClientType}>
            <Select.Trigger class="max-w-44" aria-label="Client">{clientTitle(createClientType)}</Select.Trigger>
            <Select.Content align="start">
              {#each clientTypeOptions as clientType (clientType)}
                <Select.Item value={clientType} label={clientType}>{clientType}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>

        <PromptInput.Body>
          <PromptInput.Textarea
            id="chat-prompt"
            bind:value={prompt}
            placeholder="Ask the agent to implement, inspect, or explain something…"
            class="min-h-28 text-base"
          />
        </PromptInput.Body>

        <PromptInput.Toolbar class="justify-between gap-2 pt-1">
          <Button
            type="button"
            size="sm"
            variant={taskMode ? 'default' : 'outline'}
            class="rounded-full font-normal"
            aria-pressed={taskMode}
            aria-label={taskMode ? 'Task mode on' : 'Task mode off'}
            onclick={() => (taskMode = !taskMode)}
          >
            <GitBranch class="size-4" /> Task
          </Button>
          <PromptInput.Submit disabled={!canCreate || creating} aria-label={creating ? (taskMode ? 'Creating task' : 'Starting chat') : (taskMode ? 'Create task' : 'Start chat')} />
        </PromptInput.Toolbar>
      </PromptInput.Root>
    </div>
  {:else}
    <div class="min-h-0 flex-1">
      <div class="flex h-full min-h-0 flex-col overflow-hidden rounded-xl bg-transparent">
        {#if $sessionDetailLoading && !selectedSession}
          <div class="space-y-4 p-6"><Skeleton class="h-10 w-1/3" /><Skeleton class="h-80 w-full" /></div>
        {:else if !selectedSession}
          <Empty.Root class="h-full">
            <Empty.Header>
              <Empty.Title>Session not found</Empty.Title>
              <Empty.Description>Start a new chat or select a recent session from the sidebar.</Empty.Description>
            </Empty.Header>
            <Empty.Content><Button onclick={openNewChat}>Start a new chat</Button></Empty.Content>
          </Empty.Root>
        {:else}
          {#if plannerTaskId}
            <div class="border-b p-4">
              <div class="rounded-xl border bg-card p-4 shadow-sm">
                <div class="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 class="text-lg font-semibold">Planner draft DAG</h3>
                    <p class="text-sm text-muted-foreground">Task {plannerTaskId}</p>
                  </div>
                  {#if $taskProposalsLoading}
                    <span class="text-sm text-muted-foreground">Loading proposal…</span>
                  {:else if draftPlannerProposal}
                    <span class="rounded-full border px-2.5 py-1 text-xs text-muted-foreground">revision {draftPlannerProposal.revision} · {draftPlannerProposal.state}</span>
                  {/if}
                </div>

                {#if draftPlannerProposal}
                  {@const draftWorkItems = proposalWorkItems(draftPlannerProposal)}
                  {@const draftEdges = proposalEdges(draftPlannerProposal)}
                  <p class="mt-3 text-sm">{draftPlannerProposal.summary}</p>
                  <div class="mt-4 grid gap-3 md:grid-cols-2">
                    {#each draftWorkItems as item}
                      <div class="rounded-lg border bg-background p-3">
                        <div class="font-medium">{stringField(item, 'title')}</div>
                        <div class="mt-1 text-sm text-muted-foreground">{stringField(item, 'description')}</div>
                        <div class="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                          <span>{stringField(item, 'temp_id', stringField(item, 'work_item_id', 'draft'))}</span>
                          <span>{stringField(item, 'kind')}</span>
                          <span>profile {stringField(item, 'execution_profile_id')}</span>
                        </div>
                      </div>
                    {/each}
                  </div>
                  {#if draftEdges.length}
                    <div class="mt-4 space-y-1 text-sm text-muted-foreground">
                      {#each draftEdges as edge}
                        <div>{stringField(edge, 'from_work_item_id')} → {stringField(edge, 'to_work_item_id')} <span class="text-xs">{stringField(edge, 'edge_type', 'depends_on')}</span></div>
                      {/each}
                    </div>
                  {/if}
                {:else if !$taskProposalsLoading}
                  <p class="mt-3 text-sm text-muted-foreground">Waiting for the planner to submit a draft DAG proposal.</p>
                {/if}
              </div>
            </div>
          {/if}

          <SessionConversation {messages} loading={$sessionDetailLoading} />

          <div class="shrink-0 border-t bg-background/95 p-4 backdrop-blur supports-[backdrop-filter]:bg-background/80">
            <div class="mb-2 flex min-w-0 flex-wrap items-center gap-2 px-2">
              <Badge variant="secondary" class="h-7 gap-1.5 px-3 text-sm">
                <Activity class="size-4" /> {selectedSession.state}
              </Badge>
              <span class="min-w-0 truncate rounded-full border bg-background px-3 py-1 text-sm text-muted-foreground" title={sessionWorkspacePath(selectedSession)}>
                {sessionWorkspacePath(selectedSession)}
              </span>
              <span class="text-sm text-muted-foreground">Client: {selectedSession.client_type}</span>
              <span class="text-sm text-muted-foreground">Profile: {sessionProfileTitle(selectedSession)}</span>
              <span class="text-sm text-muted-foreground">Handle: {selectedSession.handle ?? '—'}</span>
            </div>
            <div class="mb-2 flex items-center justify-end gap-2 px-2">
              {#if !isTerminalChatSession(selectedSession)}
                <Button variant="destructive" size="sm" disabled={actionBusy} aria-label="Exit session" onclick={() => void runSessionLifecycle('exit')}><LogOut class="size-4" /> Exit</Button>
              {/if}
              <div class="relative">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={actionBusy}
                  aria-haspopup="menu"
                  aria-expanded={advancedControlsOpen}
                  aria-label="Advanced session controls"
                  onclick={() => (advancedControlsOpen = !advancedControlsOpen)}
                >
                  <MoreHorizontal class="size-4" /> Advanced
                </Button>
                {#if advancedControlsOpen}
                  <div role="menu" class="absolute right-0 z-10 mt-1 w-48 rounded-lg border bg-popover p-1 text-popover-foreground shadow-md">
                    <button
                      type="button"
                      role="menuitem"
                      class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted"
                      onclick={() => {
                        advancedControlsOpen = false
                        openSessionConsole()
                      }}
                    >
                      <TerminalSquare class="size-4" /> Session Console
                    </button>
                    <button
                      type="button"
                      role="menuitem"
                      class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted disabled:pointer-events-none disabled:opacity-50"
                      disabled={actionBusy}
                      onclick={() => {
                        advancedControlsOpen = false
                        void runSessionLifecycle('restart')
                      }}
                    >
                      <RotateCw class="size-4" /> Restart session
                    </button>
                  </div>
                {/if}
              </div>
            </div>
            <SessionMessageComposer
              bind:value={input}
              busy={submitting}
              disabled={!canSendSessionMessage(selectedSession, 'x') || submitting}
              submitDisabled={!canSend}
              onValueChange={(value) => (input = value)}
              onSubmit={() => void sendMessage()}
            />
            {#if selectedSession.state === 'exited'}
              <p class="mt-2 text-xs text-muted-foreground">Sending a message will resume this session automatically.</p>
            {:else if canSendSessionMessage(selectedSession, 'x') === false}
              <p class="mt-2 text-xs text-muted-foreground">This session cannot accept new messages.</p>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>
