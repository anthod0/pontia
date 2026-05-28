<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, MessageCircle, SquarePen, TerminalSquare } from '@lucide/svelte'
  import { getPathParams, navigate } from 'svelte-mini-router'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import SessionMessageComposer from '$lib/components/session-chat/SessionMessageComposer.svelte'
  import type { AgentProfileView, SessionView, WorkspaceView } from '../api/types'
  import {
    canSendSessionMessage,
    sessionChatTitle,
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
    sessionDetail,
    sessionDetailError,
    sessionDetailLoading,
    sessions,
    sessionsError,
    submitInboxMessage,
  } from '../stores/sessions'

  let selectedSessionId = ''
  let prompt = ''
  let createWorkspaceId = ''
  let createProfileId = ''
  let createClientType = 'pi'
  let creating = false

  let input = ''
  let submitting = false
  let actionError: string | null = null
  let actionMessage: string | null = null

  onMount(async () => {
    selectedSessionId = requestedSessionIdFromLocation()
    await Promise.all([loadSessions(), loadWorkspaces(), loadAgentProfiles()])
    if (!createWorkspaceId && $workspaces.length) createWorkspaceId = $workspaces[0].workspace_id
    if (selectedSessionId) await loadSessionDetail(selectedSessionId)
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
  $: errorMessage = actionError ?? $sessionDetailError ?? $sessionsError ?? $workspacesError ?? $agentProfilesError

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

  function sessionWorkspaceTitle(session: SessionView): string {
    return session.workspace_id ?? session.workspace ?? 'No workspace'
  }

  function openSessionConsole(): void {
    navigate(selectedSessionId ? `/sessions/${selectedSessionId}` : '/sessions')
  }

  function openNewChat(): void {
    selectedSessionId = ''
    actionError = null
    actionMessage = null
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
    actionMessage = null
    if (selectedSessionId) await loadSessionDetail(selectedSessionId)
  }

  async function startChat(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    actionMessage = null
    try {
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
      actionMessage = 'Session created and initial prompt queued.'
      navigate(`/chat/${result.session.session_id}`)
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
  }

  async function sendMessage(): Promise<void> {
    if (!canSend || !selectedSessionId) return
    submitting = true
    actionError = null
    actionMessage = null
    const message = input.trim()
    try {
      await submitInboxMessage(selectedSessionId, {
        input: message,
        delivery_policy: 'after_idle',
        metadata: { source: 'dashboard_chat' },
      })
      input = ''
      actionMessage = 'Message queued for the selected session.'
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      submitting = false
    }
  }
</script>

<svelte:window onpopstate={() => void selectSessionFromLocation()} />

<section class="flex h-[calc(100vh-5rem)] min-h-[42rem] flex-col gap-4">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight">
        <MessageCircle class="size-7" /> {selectedSession ? sessionChatTitle(selectedSession) : 'New Chat'}
      </h2>
      {#if selectedSession}
        <div class="flex max-w-5xl flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground">
          <span>Client: {selectedSession.client_type}</span>
          <span>Profile: {sessionProfileTitle(selectedSession)}</span>
          <span>Handle: {selectedSession.handle ?? '—'}</span>
          <span>Description: {selectedSession.description ?? '—'}</span>
          <span>State: {selectedSession.state}</span>
          <span>Workspace: {sessionWorkspaceTitle(selectedSession)}</span>
        </div>
      {:else}
        <p class="max-w-3xl text-muted-foreground">Start a new agent session from a prompt, workspace, client, and profile.</p>
      {/if}
    </div>
    <div class="flex gap-2">
      {#if selectedSessionId}
        <Button variant="outline" onclick={openNewChat}><SquarePen class="size-4" /> New chat</Button>
      {/if}
      <Button variant="outline" onclick={openSessionConsole}><TerminalSquare class="size-4" /> Session Console</Button>
    </div>
  </div>

  {#if errorMessage}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Chat error</Alert.Title>
      <Alert.Description>{errorMessage}</Alert.Description>
    </Alert.Root>
  {/if}
  {#if actionMessage}
    <Alert.Root>
      <Alert.Title>Chat updated</Alert.Title>
      <Alert.Description>{actionMessage}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if !selectedSessionId}
    <div class="flex min-h-0 flex-1 items-center justify-center">
      <PromptInput.Root class="w-full max-w-4xl space-y-3" onSubmit={() => void startChat()}>
        <PromptInput.Body>
          <PromptInput.Textarea
            id="chat-prompt"
            bind:value={prompt}
            placeholder="Ask the agent to implement, inspect, or explain something…"
            class="min-h-40 text-base"
          />
        </PromptInput.Body>

        <PromptInput.Toolbar class="gap-2 pt-1">
          <div class="flex min-w-0 flex-1 flex-wrap items-center gap-2">
            <Select.Root type="single" bind:value={createWorkspaceId} disabled={$workspacesLoading}>
              <Select.Trigger class="max-w-56 border-none" aria-label="Workspace" title={selectedWorkspace?.canonical_path ?? undefined}>
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
              <Select.Trigger class="max-w-56 border-none" aria-label="Profile">
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
              <Select.Trigger class="max-w-44 border-none" aria-label="Client">{clientTitle(createClientType)}</Select.Trigger>
              <Select.Content align="start">
                {#each clientTypeOptions as clientType (clientType)}
                  <Select.Item value={clientType} label={clientType}>{clientType}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>

          <PromptInput.Submit disabled={!canCreate || creating} aria-label={creating ? 'Starting chat' : 'Start chat'} />
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
          <SessionConversation {messages} loading={$sessionDetailLoading} />

          <div class="p-4">
            <SessionMessageComposer
              bind:value={input}
              busy={submitting}
              disabled={!canSendSessionMessage(selectedSession, 'x') || submitting}
              submitDisabled={!canSend}
              onValueChange={(value) => (input = value)}
              onSubmit={() => void sendMessage()}
            />
            {#if canSendSessionMessage(selectedSession, 'x') === false}
              <p class="mt-2 text-xs text-muted-foreground">This session is closed; start a new chat to continue.</p>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>
