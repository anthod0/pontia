<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, MessageCircle, TerminalSquare } from '@lucide/svelte'
  import { navigate } from 'svelte-mini-router'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import SessionMessageComposer from '$lib/components/session-chat/SessionMessageComposer.svelte'
  import {
    canSendSessionMessage,
    sessionChatTitle,
    turnsToChatMessages,
    visibleChatSessions,
  } from '$lib/session-chat/sessionChat'
  import {
    loadSessionDetail,
    loadSessions,
    sessionDetail,
    sessionDetailError,
    sessionDetailLoading,
    sessions,
    sessionsError,
    sessionsLoading,
    submitInboxMessage,
  } from '../stores/sessions'

  let selectedSessionId = ''
  let input = ''
  let submitting = false
  let actionError: string | null = null
  let actionMessage: string | null = null

  onMount(async () => {
    const loaded = await loadSessions()
    await selectSessionFromLocation(loaded)
  })

  $: selectedSession = $sessions.find((session) => session.session_id === selectedSessionId) ?? $sessionDetail?.session ?? null
  $: messages = $sessionDetail && $sessionDetail.session.session_id === selectedSessionId ? turnsToChatMessages($sessionDetail.turns) : []
  $: canSend = canSendSessionMessage(selectedSession, input) && !submitting
  $: errorMessage = actionError ?? $sessionDetailError ?? $sessionsError

  async function selectSessionFromLocation(availableSessions = $sessions): Promise<void> {
    const requestedSessionId = new URLSearchParams(window.location.search).get('session') ?? ''
    const requestedSession = availableSessions.find((session) => session.session_id === requestedSessionId)
    const nextSessionId = requestedSession?.session_id ?? visibleChatSessions(availableSessions, 'active')[0]?.session_id ?? visibleChatSessions(availableSessions, 'all')[0]?.session_id ?? ''
    if (!nextSessionId || nextSessionId === selectedSessionId) return

    selectedSessionId = nextSessionId
    input = ''
    actionError = null
    actionMessage = null
    await loadSessionDetail(nextSessionId)
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
        metadata: { source: 'dashboard_session_chat' },
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
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><MessageCircle class="size-7" /> Chat</h2>
      <p class="max-w-3xl text-muted-foreground">A focused conversation view for existing sessions. Advanced controls, events, artifacts, and debug payloads stay in Session Console.</p>
    </div>
    <div class="flex gap-2">
      <Button variant="outline" onclick={() => navigate('/sessions')}><TerminalSquare class="size-4" /> Session Console</Button>
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
      <Alert.Title>Message sent</Alert.Title>
      <Alert.Description>{actionMessage}</Alert.Description>
    </Alert.Root>
  {/if}

  <div class="min-h-0 flex-1">
    <div class="flex h-full min-h-0 flex-col overflow-hidden rounded-xl border bg-card">
      {#if $sessionsLoading && !selectedSessionId}
        <div class="space-y-4 p-6"><Skeleton class="h-10 w-1/3" /><Skeleton class="h-80 w-full" /></div>
      {:else if !$sessions.length}
        <Empty.Root class="h-full">
          <Empty.Header>
            <Empty.Media><MessageCircle class="size-7" /></Empty.Media>
            <Empty.Title>No sessions yet</Empty.Title>
            <Empty.Description>Create a session in Session Console, then return here for a focused chat view.</Empty.Description>
          </Empty.Header>
          <Empty.Content><Button onclick={() => navigate('/sessions')}>Open Session Console</Button></Empty.Content>
        </Empty.Root>
      {:else if !selectedSession}
        <Empty.Root class="h-full">
          <Empty.Header>
            <Empty.Title>Select a session</Empty.Title>
            <Empty.Description>Choose a recent active session from the global sidebar to view the conversation.</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {:else}
        <div class="border-b p-4">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 class="text-lg font-semibold">{sessionChatTitle(selectedSession)}</h3>
              <p class="text-sm text-muted-foreground">{selectedSession.workspace_id ?? selectedSession.workspace ?? 'No workspace'} · {selectedSession.client_type}</p>
            </div>
            <Badge variant="secondary">{selectedSession.state}</Badge>
          </div>
        </div>

        <SessionConversation {messages} loading={$sessionDetailLoading} />

        <div class="border-t p-4">
          <SessionMessageComposer
            bind:value={input}
            busy={submitting}
            disabled={!canSendSessionMessage(selectedSession, 'x') || submitting}
            submitDisabled={!canSend}
            onValueChange={(value) => (input = value)}
            onSubmit={() => void sendMessage()}
          />
          {#if selectedSession && canSendSessionMessage(selectedSession, 'x') === false}
            <p class="mt-2 text-xs text-muted-foreground">This session is closed; create or select an active session to continue chatting.</p>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</section>
