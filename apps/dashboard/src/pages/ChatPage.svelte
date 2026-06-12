<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { Activity, AtSign, Bot, Folder, GitBranch, LogOut, MoreHorizontal, RotateCw, Terminal, TerminalSquare } from '@lucide/svelte'
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
  import type { AgentProfileView, DashboardStreamEvent, SessionView, TurnView, WorkspaceView } from '../api/types'
  import {
    canSendSessionMessage,
    isTerminalChatSession,
    timelineItemsToChatMessages,
  } from '$lib/session-chat/sessionChat'
  import type { SessionChatMessage } from '$lib/session-chat/sessionChat'
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
  import {
    handleTimelineMessageUpdated,
    loadSessionTimeline,
    resetTimelineState,
    timelineState,
  } from '../stores/timeline'
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
  let optimisticInitialMessages: Record<string, SessionChatMessage> = {}
  let unsubscribeDashboardEvents: (() => void) | null = null

  const AUTO_RESUME_IDLE_TIMEOUT_MS = 30_000
  const newChatSelectorTriggerClass = 'h-7 rounded-full px-3 text-sm font-normal text-muted-foreground'

  onMount(async () => {
    selectedSessionId = requestedSessionIdFromLocation()
    await Promise.all([loadSessions(), loadWorkspaces(), loadAgentProfiles()])
    if (!createWorkspaceId && $workspaces.length) createWorkspaceId = $workspaces[0].workspace_id
    if (selectedSessionId) {
      resetTimelineState(selectedSessionId)
      await Promise.all([loadSessionDetail(selectedSessionId), loadSessionTimeline(selectedSessionId, { mode: 'rebuild' })])
    }
    unsubscribeDashboardEvents = subscribeDashboardEvents(handleDashboardEvent)
  })

  onDestroy(() => {
    unsubscribeDashboardEvents?.()
  })

  $: selectedSession = selectedSessionId ? ($sessions.find((session) => session.session_id === selectedSessionId) ?? $sessionDetail?.session ?? null) : null
  $: messages = chatMessagesWithOptimistic(selectedSessionId, $timelineState.sessionId === selectedSessionId ? timelineItemsToChatMessages($timelineState.items) : [])
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
  $: errorMessage = actionError ?? $sessionDetailError ?? $timelineState.error ?? $sessionsError ?? $workspacesError ?? $agentProfilesError ?? $taskProposalsError
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

  function sessionProfileTitle(session: SessionView): string | null {
    if (!session.execution_profile_id) return null
    return session.execution_profile_version ? `${session.execution_profile_id}@${session.execution_profile_version}` : session.execution_profile_id
  }

  function sessionHandleTitle(session: SessionView): string | null {
    const handle = session.handle?.trim()
    return handle || null
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

  function navigateToTaskDag(taskId: string): void {
    if (appliedRedirectTaskId === taskId) return
    appliedRedirectTaskId = taskId
    navigate(`/tasks/${taskId}/dag`)
  }

  function handleDashboardEvent(streamEvent: DashboardStreamEvent): void {
    if (streamEvent.kind === 'session_event') {
      if (streamEvent.event.session_id !== selectedSessionId) return
      if (streamEvent.event.type !== 'session.message_updated') return
      const rawBindingId = streamEvent.event.payload.binding_id
      const bindingId = typeof rawBindingId === 'string' ? rawBindingId : null
      void handleTimelineMessageUpdated(selectedSessionId, bindingId)
      return
    }

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
    resetTimelineState()
    navigate('/chat')
  }

  function rememberOptimisticInitialMessage(sessionId: string, input: string, turn: Pick<TurnView, 'turn_id' | 'created_at'> | null = null): void {
    const content = input.trim()
    if (!sessionId || !content) return
    const turnId = turn?.turn_id ?? `${sessionId}:initial`
    optimisticInitialMessages = {
      ...optimisticInitialMessages,
      [sessionId]: {
        id: `optimistic:${turnId}:user`,
        turnId,
        role: 'user',
        content,
        status: 'sent',
        createdAt: turn?.created_at ?? new Date().toISOString(),
      },
    }
  }

  function chatMessagesWithOptimistic(sessionId: string, loadedMessages: SessionChatMessage[]): SessionChatMessage[] {
    const optimistic = optimisticInitialMessages[sessionId]
    if (!optimistic) return loadedMessages
    const alreadyLoaded = loadedMessages.some((message) => (
      message.role === 'user'
      && (message.turnId === optimistic.turnId || message.content.trim() === optimistic.content.trim())
    ))
    return alreadyLoaded ? loadedMessages : [optimistic, ...loadedMessages]
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
    if (selectedSessionId) {
      resetTimelineState(selectedSessionId)
      await Promise.all([loadSessionDetail(selectedSessionId), loadSessionTimeline(selectedSessionId, { mode: 'rebuild' })])
    } else {
      resetTimelineState()
    }
  }

  function handleNewChatKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      void startChat()
    }
  }

  async function startChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    try {
      if (taskMode) {
        const initialPrompt = prompt.trim()
        const result = await createDagTask({
          input: initialPrompt,
          workspace: selectedWorkspace?.canonical_path ?? selectedWorkspace?.display_path ?? createWorkspaceId,
          client_type: createClientType.trim() || 'pi',
          metadata: { source: 'dashboard_chat', action: 'manual_task' },
        })
        selectedSessionId = result.planning_turn.session_id
        rememberOptimisticInitialMessage(result.planning_turn.session_id, initialPrompt, {
          turn_id: result.planning_turn.turn_id,
          created_at: new Date().toISOString(),
        })
        prompt = ''
        resetTimelineState(result.planning_turn.session_id)
        navigate(`/chat/${result.planning_turn.session_id}`)
        await Promise.all([loadSessionDetail(result.planning_turn.session_id), loadSessionTimeline(result.planning_turn.session_id, { mode: 'rebuild' })])
        return
      }

      const initialPrompt = prompt.trim()
      const result = await createSession({
        client_type: createClientType.trim(),
        workspace_id: createWorkspaceId,
        handle: defaultHandleForProfile(selectedProfile) || null,
        role: selectedProfile?.default_session_role ?? null,
        description: selectedProfile?.default_session_description ?? null,
        ...sessionProfileFields(selectedProfile),
        initial_task: { input: initialPrompt, metadata: { source: 'dashboard_chat' } },
        metadata: { source: 'dashboard_chat' },
      })
      selectedSessionId = result.session.session_id
      rememberOptimisticInitialMessage(result.session.session_id, initialPrompt, result.initial_turn)
      prompt = ''
      resetTimelineState(result.session.session_id)
      navigate(`/chat/${result.session.session_id}`)
      await Promise.all([loadSessionDetail(result.session.session_id), loadSessionTimeline(result.session.session_id, { mode: 'rebuild' })])
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

  function sessionStateFromStores(sessionId: string): string | null {
    const detail = get(sessionDetail)
    if (detail?.session.session_id === sessionId) return detail.session.state
    return get(sessions).find((session) => session.session_id === sessionId)?.state ?? null
  }

  function waitForSessionIdle(sessionId: string, timeoutMs = AUTO_RESUME_IDLE_TIMEOUT_MS): Promise<void> {
    if (sessionStateFromStores(sessionId) === 'idle') return Promise.resolve()

    return new Promise((resolve, reject) => {
      let done = false
      let unsubscribeSessions: (() => void) | null = null
      let unsubscribeDetail: (() => void) | null = null

      const cleanup = () => {
        unsubscribeSessions?.()
        unsubscribeDetail?.()
        clearTimeout(timeout)
      }
      const finish = (callback: () => void) => {
        if (done) return
        done = true
        cleanup()
        callback()
      }
      const check = () => {
        if (sessionStateFromStores(sessionId) === 'idle') finish(resolve)
      }
      const timeout = setTimeout(() => {
        finish(() => reject(new Error('Session resume timed out before becoming idle.')))
      }, timeoutMs)

      unsubscribeSessions = sessions.subscribe(check)
      unsubscribeDetail = sessionDetail.subscribe(check)
      check()
    })
  }

  async function sendMessage(): Promise<void> {
    if (!canSend || !selectedSessionId) return
    submitting = true
    actionError = null
    const message = input.trim()
    try {
      if (selectedSession?.state === 'exited') {
        await resumeSession(selectedSessionId)
        await waitForSessionIdle(selectedSessionId)
      }
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

<section class={selectedSessionId ? 'flex flex-col gap-4 pb-40' : 'flex min-h-[calc(100vh-9.5rem)] flex-col'}>
  {#if !selectedSessionId}
    <div data-testid="new-chat-centered-panel" class="flex min-h-0 flex-1 flex-col justify-center">
      <div class="mx-auto w-full max-w-4xl space-y-6">
        <div class="space-y-2">
          <h2 class="text-3xl font-semibold tracking-tight">New Chat</h2>
          <p class="max-w-3xl text-muted-foreground">Start a new agent session from a prompt, workspace, client, and profile.</p>
        </div>

        <div class="space-y-3">
          <div class="flex min-w-0 flex-wrap items-center gap-2 px-1">
          <Button
            type="button"
            size="sm"
            variant={taskMode ? 'default' : 'outline'}
            class="h-7 rounded-full px-3 text-sm font-normal"
            aria-pressed={taskMode}
            aria-label={taskMode ? 'Task mode on' : 'Task mode off'}
            onclick={() => (taskMode = !taskMode)}
          >
            <GitBranch class="size-4" /> Task
          </Button>

          <Select.Root type="single" bind:value={createWorkspaceId} disabled={$workspacesLoading}>
            <Select.Trigger class={`${newChatSelectorTriggerClass} max-w-56`} aria-label="Workspace" title={selectedWorkspace?.canonical_path ?? undefined}>
              <Folder class="size-4" aria-hidden="true" />
              <span class="min-w-0 truncate">{#if selectedWorkspace}{workspaceTitle(selectedWorkspace)}{:else}Workspace{/if}</span>
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

          <Select.Root type="single" bind:value={createClientType}>
            <Select.Trigger class={`${newChatSelectorTriggerClass} max-w-44`} aria-label="Client">
              <Terminal class="size-4" aria-hidden="true" />
              <span class="min-w-0 truncate">{clientTitle(createClientType)}</span>
            </Select.Trigger>
            <Select.Content align="start">
              {#each clientTypeOptions as clientType (clientType)}
                <Select.Item value={clientType} label={clientType}>{clientType}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>

          <PromptInput.Root class="w-full" onSubmit={() => void startChat()}>
            <PromptInput.Body>
              <PromptInput.Textarea
                id="chat-prompt"
                bind:value={prompt}
                placeholder="Ask the agent to implement, inspect, or explain something…"
                onkeydown={handleNewChatKeydown}
              />
            </PromptInput.Body>

            <PromptInput.Toolbar class="justify-between gap-2 pt-1">
              <p class="px-2 text-xs text-muted-foreground">Enter to send · Shift+Enter for newline</p>
              <PromptInput.Submit disabled={!canCreate || creating} aria-label={creating ? (taskMode ? 'Creating task' : 'Starting chat') : (taskMode ? 'Create task' : 'Start chat')} />
            </PromptInput.Toolbar>
          </PromptInput.Root>
        </div>
      </div>
    </div>
  {:else}
    <div class="flex-1">
      <div class="flex flex-col rounded-xl bg-transparent">
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
          <SessionConversation
            {messages}
            sessionState={selectedSession.state}
            loading={($sessionDetailLoading || $timelineState.loading) && !messages.length}
            {plannerTaskId}
            {draftPlannerProposal}
            draftPlannerProposalLoading={$taskProposalsLoading}
          />

          <div data-chat-composer-dock="fixed" class="fixed bottom-0 left-0 right-0 z-30 border-t bg-background/95 p-4 backdrop-blur supports-[backdrop-filter]:bg-background/80 md:left-[var(--sidebar-width)] md:p-6">
            <div class="mx-auto w-full max-w-7xl">
            <div role="group" aria-label="Session status and controls" class="mb-2 flex min-w-0 flex-wrap items-center justify-between gap-2 px-2">
              <div class="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                <Badge variant="secondary" class="h-7 gap-1.5 px-3 text-sm">
                  <Activity class="size-4" /> {selectedSession.state}
                </Badge>
                <Badge
                  variant="outline"
                  class="h-7 max-w-full justify-start gap-1.5 px-3 text-sm font-normal text-muted-foreground"
                  title={`Workspace: ${sessionWorkspacePath(selectedSession)}`}
                  aria-label={`Workspace: ${sessionWorkspacePath(selectedSession)}`}
                >
                  <Folder class="size-4" aria-hidden="true" />
                  <span class="min-w-0 truncate">{sessionWorkspacePath(selectedSession)}</span>
                </Badge>
                <Badge
                  variant="outline"
                  class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground"
                  title={`Client: ${selectedSession.client_type}`}
                  aria-label={`Client: ${selectedSession.client_type}`}
                >
                  <Terminal class="size-4" aria-hidden="true" /> {selectedSession.client_type}
                </Badge>
                {#if sessionProfileTitle(selectedSession)}
                  <Badge
                    variant="outline"
                    class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground"
                    title={`Profile: ${sessionProfileTitle(selectedSession)}`}
                    aria-label={`Profile: ${sessionProfileTitle(selectedSession)}`}
                  >
                    <Bot class="size-4" aria-hidden="true" /> {sessionProfileTitle(selectedSession)}
                  </Badge>
                {/if}
                {#if sessionHandleTitle(selectedSession)}
                  <Badge
                    variant="outline"
                    class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground"
                    title={`Handle: ${sessionHandleTitle(selectedSession)}`}
                    aria-label={`Handle: ${sessionHandleTitle(selectedSession)}`}
                  >
                    <AtSign class="size-4" aria-hidden="true" /> {sessionHandleTitle(selectedSession)}
                  </Badge>
                {/if}
              </div>
              <div class="flex shrink-0 items-center justify-end gap-2">
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
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>
