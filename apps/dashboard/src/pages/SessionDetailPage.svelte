<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, MessageCircle, RefreshCw, Send, TerminalSquare } from '@lucide/svelte'
  import { navigate } from '$lib/navigation'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import { formatDateTime, jsonPreview, shortId } from '../components/tasks/format'
  import { contextUsageRatio, contextUsageSummary } from '$lib/contextUsage'
  import type { ContextUsageView, InboxDeliveryPolicy, SessionView } from '../api/types'
  import { selectCurrentTurnOutput } from './sessions/currentTurnOutput'
  import { capabilityRows, extraCapabilityRows } from './sessions/sessionCapabilities'
  import { sessionEventDetailRows, sessionEventSummary, sessionEventTurnLabel } from './sessions/sessionEvents'
  import { isTerminalSession, sessionDisplayTitle } from './sessions/sessionList'
  import {
    interruptSession,
    loadSessionDetail,
    loadSessions,
    restartSession,
    sessionDetail,
    sessionDetailError,
    sessionDetailLoading,
    submitInboxMessage,
    terminateSession,
  } from '../stores/sessions'

  export let routeSessionId: string | null = null

  let selectedSessionId = ''
  let actionError: string | null = null
  let actionMessage: string | null = null
  let actionBusy = false

  let inboxInput = ''
  let inboxPolicy: InboxDeliveryPolicy = 'after_idle'
  let submittingInbox = false

  onMount(() => {
    void loadSelectedSession()
  })

  $: selectedSession = $sessionDetail?.session ?? null
  $: currentTurnOutput = $sessionDetail ? selectCurrentTurnOutput($sessionDetail.session, $sessionDetail.turns) : null
  $: inboxSubmitReason = inboxSubmitUnsupportedReason(selectedSession)
  $: canSubmitInbox = Boolean(selectedSessionId && inboxInput.trim() && !submittingInbox && !inboxSubmitReason)
  $: interruptReason = interruptUnsupportedReason(selectedSession)
  $: restartReason = selectedSession && isTerminalSession(selectedSession) ? 'Terminal sessions cannot be restarted.' : null
  $: terminateReason = selectedSession && isTerminalSession(selectedSession) ? 'Session is already terminal.' : null

  function sessionTitle(session: SessionView): string {
    return sessionDisplayTitle(session)
  }

  function contextUsageTone(usage: ContextUsageView): string {
    const ratio = contextUsageRatio(usage)
    if (ratio === null || ratio < 0.7) return 'bg-emerald-500'
    if (ratio <= 0.9) return 'bg-amber-500'
    return 'bg-destructive'
  }

  function inboxSubmitUnsupportedReason(session: SessionView | null): string | null {
    if (!session) return 'Select a session first.'
    if (session.capabilities?.accept_task !== true) return '此 session 当前不可从 Web 写入'
    return null
  }

  function interruptUnsupportedReason(session: SessionView | null): string | null {
    if (!session) return 'Select a session first.'
    if (!session.capabilities?.interrupt) return 'Selected session runtime does not advertise interrupt capability.'
    if (session.state !== 'busy') return 'Selected session has no running turn to interrupt.'
    return null
  }

  function requestedSessionIdFromLocation(): string {
    if (routeSessionId) return routeSessionId
    const pathMatch = window.location.pathname.match(/\/sessions\/([^/?#]+)$/)
    return (pathMatch ? decodeURIComponent(pathMatch[1]) : null) ?? new URLSearchParams(window.location.search).get('session') ?? ''
  }

  async function loadSelectedSession(): Promise<void> {
    const nextSessionId = requestedSessionIdFromLocation()
    selectedSessionId = nextSessionId
    actionError = null
    actionMessage = null
    if (nextSessionId) {
      await loadSessionDetail(nextSessionId)
    }
  }

  function openSelectedSessionChat(): void {
    if (selectedSessionId) navigate(`/chat/${selectedSessionId}`)
    else navigate('/chat')
  }

  async function refreshAll(): Promise<void> {
    actionError = null
    actionMessage = null
    await Promise.all([
      loadSessions(),
      selectedSessionId ? loadSessionDetail(selectedSessionId) : Promise.resolve(null),
    ])
  }

  async function submitInbox(): Promise<void> {
    if (!canSubmitInbox) return
    submittingInbox = true
    actionError = null
    actionMessage = null
    try {
      await submitInboxMessage(selectedSessionId, {
        input: inboxInput.trim(),
        delivery_policy: inboxPolicy,
        metadata: { source: 'dashboard_session_detail' },
      })
      inboxInput = ''
      actionMessage = 'Inbox message submitted.'
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      submittingInbox = false
    }
  }

  async function runControl(action: 'interrupt' | 'restart' | 'terminate'): Promise<void> {
    if (!selectedSessionId) return
    actionBusy = true
    actionError = null
    actionMessage = null
    try {
      if (action === 'interrupt') await interruptSession(selectedSessionId)
      if (action === 'restart') await restartSession(selectedSessionId)
      if (action === 'terminate') await terminateSession(selectedSessionId)
      actionMessage = `Session ${action} request accepted.`
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      actionBusy = false
    }
  }
</script>

<svelte:window onpopstate={() => void loadSelectedSession()} />

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><TerminalSquare class="size-7" /> Session detail</h2>
    </div>
    <div class="flex gap-2">
      <Button variant="outline" onclick={() => navigate('/sessions')}>Back to Sessions</Button>
      <Button variant="outline" disabled={selectedSession?.capabilities?.timeline !== true} onclick={openSelectedSessionChat}><MessageCircle class="size-4" /> Open Chat</Button>
      <Button variant="outline" onclick={() => void refreshAll()}><RefreshCw class="size-4" /> Refresh</Button>
    </div>
  </div>

  {#if $sessionDetailError || actionError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Session detail error</Alert.Title>
      <Alert.Description>{actionError ?? $sessionDetailError}</Alert.Description>
    </Alert.Root>
  {/if}
  {#if actionMessage}
    <Alert.Root>
      <Alert.Title>Action completed</Alert.Title>
      <Alert.Description>{actionMessage}</Alert.Description>
    </Alert.Root>
  {/if}

{#if $sessionDetailLoading}
  <div class="space-y-4"><Skeleton class="h-48 w-full" /><Skeleton class="h-80 w-full" /><Skeleton class="h-80 w-full" /></div>
{:else if !$sessionDetail}
  <Empty.Root>
    <Empty.Header><Empty.Title>Select a session</Empty.Title><Empty.Description>Choose a session to inspect metadata, turns, inbox, events, and output references.</Empty.Description></Empty.Header>
  </Empty.Root>
{:else}
  <Tabs.Root value="messages" class="space-y-4">
    <Tabs.List>
      <Tabs.Trigger value="messages">Messages</Tabs.Trigger>
      <Tabs.Trigger value="details">Details</Tabs.Trigger>
      <Tabs.Trigger value="events">Events</Tabs.Trigger>
    </Tabs.List>

    <Tabs.Content value="details" class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
      <Card.Root>
        <Card.Header>
          <Card.Title>{sessionTitle($sessionDetail.session)}</Card.Title>
          <Card.Description>{$sessionDetail.session.description ?? 'No session description.'}</Card.Description>
        </Card.Header>
        <Card.Content class="space-y-4">
          <div class="grid gap-3 text-sm md:grid-cols-2 xl:grid-cols-3">
            {#each [
              ['Session ID', $sessionDetail.session.session_id],
              ['Client', $sessionDetail.session.client_type],
              ['State', $sessionDetail.session.state],
              ['Workspace', $sessionDetail.session.workspace_id ?? $sessionDetail.session.workspace ?? '—'],
              ['Profile', $sessionDetail.session.execution_profile_id ?? '—'],
              ['Current branch turn', $sessionDetail.session.current_turn_id ?? '—'],
            ] as [label, value]}
              <div class="rounded-lg border p-3"><div class="text-xs uppercase tracking-wide text-muted-foreground">{label}</div><div class="mt-1 break-words font-medium">{value}</div></div>
            {/each}
          </div>
          <div class="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" disabled={actionBusy || Boolean(interruptReason)} title={interruptReason ?? 'Interrupt current turn'} onclick={() => void runControl('interrupt')}>Interrupt</Button>
            <Button size="sm" variant="outline" disabled={actionBusy || Boolean(restartReason)} title={restartReason ?? 'Restart session'} onclick={() => void runControl('restart')}>Restart</Button>
            <Button size="sm" variant="destructive" disabled={actionBusy || Boolean(terminateReason)} title={terminateReason ?? 'Terminate session'} onclick={() => void runControl('terminate')}>Terminate/exit</Button>
          </div>
          {#if interruptReason || restartReason || terminateReason}
            <p class="text-xs text-muted-foreground">Unsupported/degraded controls: {interruptReason ?? restartReason ?? terminateReason}</p>
          {/if}
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header><Card.Title>Capabilities</Card.Title></Card.Header>
        <Card.Content class="space-y-4">
          <div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-1">
            {#each capabilityRows($sessionDetail.session.capabilities) as capability}
              <div class="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm">
                <span class="font-medium">{capability.label}</span>
                <Badge variant={capability.supported ? 'default' : 'secondary'}>{capability.value}</Badge>
              </div>
            {/each}
          </div>
          {@const extraCapabilities = extraCapabilityRows($sessionDetail.session.capabilities)}
          {#if extraCapabilities.length}
            <div class="space-y-2 border-t pt-3">
              <div class="text-xs uppercase tracking-wide text-muted-foreground">Additional</div>
              {#each extraCapabilities as [key, value]}
                <div class="flex items-start justify-between gap-3 text-sm">
                  <span class="text-muted-foreground">{key}</span>
                  <span class="break-all text-right font-mono text-xs">{value}</span>
                </div>
              {/each}
            </div>
          {/if}
        </Card.Content>
      </Card.Root>
    </Tabs.Content>

    <Tabs.Content value="messages" class="space-y-4">
      <Card.Root>
        <Card.Header><Card.Title>Turns</Card.Title><Card.Description>{$sessionDetail.turns.length} turns with output.</Card.Description></Card.Header>
        <Card.Content>
          {#if $sessionDetail.turns.length}
            <div class="overflow-x-auto">
              <Table.Root>
                <Table.Header><Table.Row><Table.Head>Turn</Table.Head><Table.Head>State</Table.Head><Table.Head>Input</Table.Head><Table.Head>Output</Table.Head><Table.Head>Completed</Table.Head></Table.Row></Table.Header>
                <Table.Body>
                  {#each $sessionDetail.turns as turn}
                    <Table.Row>
                      <Table.Cell class="font-medium">{shortId(turn.turn_id)}</Table.Cell>
                      <Table.Cell><Badge variant="secondary">{turn.state}</Badge></Table.Cell>
                      <Table.Cell class="max-w-xs truncate">{turn.input?.summary ?? jsonPreview(turn.input)}</Table.Cell>
                      <Table.Cell class="max-w-xs truncate">{turn.output?.summary ?? (turn.failure ? jsonPreview(turn.failure) : '—')}</Table.Cell>
                      <Table.Cell>{turn.completed_at ? formatDateTime(turn.completed_at) : '—'}</Table.Cell>
                    </Table.Row>
                  {/each}
                </Table.Body>
              </Table.Root>
            </div>
          {:else}
            <Empty.Root><Empty.Header><Empty.Title>No turns</Empty.Title><Empty.Description>This session has no turn history yet.</Empty.Description></Empty.Header></Empty.Root>
          {/if}
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header><Card.Title>Submit input</Card.Title></Card.Header>
        <Card.Content class="space-y-3">
          <Textarea id="inbox-input" aria-label="Inbox message" bind:value={inboxInput} placeholder="Send follow-up instructions…" disabled={Boolean(inboxSubmitReason)} />
          {#if inboxSubmitReason}<p class="text-xs text-muted-foreground">{inboxSubmitReason}</p>{/if}
          <div class="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-end">
            <div class="w-full space-y-2 sm:w-48">
              <Label for="inbox-policy">Delivery policy</Label>
              <select id="inbox-policy" bind:value={inboxPolicy} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
                <option value="after_idle">after_idle</option>
                <option value="interrupt_now">interrupt_now</option>
              </select>
            </div>
            <Button class="sm:mb-0" onclick={submitInbox} disabled={!canSubmitInbox}><Send class="size-4" /> {submittingInbox ? 'Submitting…' : 'Submit inbox message'}</Button>
          </div>
          {#if inboxPolicy === 'interrupt_now' && !$sessionDetail.session.capabilities?.interrupt}
            <p class="text-xs text-muted-foreground">This session may not support immediate interruption; the message may be queued or fail.</p>
          {/if}
        </Card.Content>
      </Card.Root>

      <div class="grid gap-4 xl:grid-cols-2">
        <Card.Root>
          <Card.Header>
            <Card.Title>Context usage</Card.Title>
            <Card.Description>Latest session-level context window usage reported by the agent client.</Card.Description>
          </Card.Header>
          <Card.Content class="space-y-3">
            {#if ($sessionDetail.session.capabilities?.context_usage ?? 'unsupported') === 'unsupported'}
              <p class="text-sm text-muted-foreground">Context usage not supported by this client.</p>
            {:else if !$sessionDetail.session.context_usage}
              <p class="text-sm text-muted-foreground">Waiting for context usage...</p>
            {:else}
              {@const usage = $sessionDetail.session.context_usage}
              {@const ratio = contextUsageRatio(usage)}
              <div class="space-y-2">
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <div class="font-medium">{contextUsageSummary(usage)}</div>
                  {#if $sessionDetail.session.model}<Badge variant="secondary">{$sessionDetail.session.model}</Badge>{/if}
                </div>
                {#if ratio !== null}
                  <div class="h-2 overflow-hidden rounded-full bg-muted" aria-label="Context usage progress">
                    <div class={`h-full ${contextUsageTone(usage)}`} style={`width: ${Math.min(100, Math.max(0, ratio * 100))}%`}></div>
                  </div>
                {/if}
                <p class="text-xs text-muted-foreground">Observed {formatDateTime(usage.observed_at)}</p>
              </div>
            {/if}
          </Card.Content>
        </Card.Root>

        <Card.Root>
          <Card.Header><Card.Title>Inbox</Card.Title><Card.Description>{$sessionDetail.inboxMessages.length} messages.</Card.Description></Card.Header>
          <Card.Content class="space-y-3">
            {#if $sessionDetail.inboxMessages.length}
              {#each $sessionDetail.inboxMessages.slice().reverse() as message}
                <div class="rounded-lg border p-3 text-sm">
                  <div class="flex flex-wrap items-center justify-between gap-2"><span class="font-medium">{message.input.summary}</span><Badge variant="secondary">{message.state}</Badge></div>
                  <div class="mt-1 text-xs text-muted-foreground">{message.delivery_policy} · turn {shortId(message.turn_id)} · {formatDateTime(message.updated_at)}</div>
                  {#if message.failure_message}<p class="mt-2 text-xs text-destructive">{message.failure_message}</p>{/if}
                </div>
              {/each}
            {:else}
              <Empty.Root><Empty.Header><Empty.Title>No inbox messages</Empty.Title><Empty.Description>Submit a message above to queue follow-up input.</Empty.Description></Empty.Header></Empty.Root>
            {/if}
          </Card.Content>
        </Card.Root>
      </div>
    </Tabs.Content>

    <Tabs.Content value="events" class="space-y-4">
      <Card.Root>
        <Card.Header><Card.Title>Session events</Card.Title><Card.Description>{$sessionDetail.events.length} events shown as compact log lines. Expand a row for full details.</Card.Description></Card.Header>
        <Card.Content>
          {#if $sessionDetail.events.length}
            <div class="overflow-hidden rounded-lg border">
              {#each $sessionDetail.events.slice(0, 50) as event}
                <details class="group border-b last:border-b-0">
                  <summary class="grid cursor-pointer list-none gap-2 px-3 py-2 text-sm hover:bg-muted/50 md:grid-cols-[11rem_minmax(10rem,16rem)_7rem_7rem_minmax(0,1fr)] md:items-center">
                    <span class="font-mono text-xs text-muted-foreground">{formatDateTime(event.time)}</span>
                    <span class="min-w-0 truncate font-medium" title={event.type}>{event.type}</span>
                    <span class="truncate text-xs text-muted-foreground" title={event.source}>{event.source}</span>
                    <span class="font-mono text-xs text-muted-foreground">turn {sessionEventTurnLabel(event.turn_id)}</span>
                    <span class="min-w-0 truncate text-muted-foreground" title={sessionEventSummary(event.payload)}>{sessionEventSummary(event.payload)}</span>
                  </summary>
                  <div class="space-y-3 border-t bg-muted/20 p-3 text-sm">
                    <div class="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
                      {#each sessionEventDetailRows(event) as [label, value]}
                        <div class="rounded-md border bg-background p-2">
                          <div class="text-[0.7rem] uppercase tracking-wide text-muted-foreground">{label}</div>
                          <div class="mt-1 break-words font-mono text-xs">{value}</div>
                        </div>
                      {/each}
                    </div>
                    <div>
                      <div class="mb-1 text-xs uppercase tracking-wide text-muted-foreground">Raw payload</div>
                      <pre class="max-h-64 overflow-auto whitespace-pre-wrap rounded bg-background p-2 text-xs">{JSON.stringify(event.payload, null, 2)}</pre>
                    </div>
                  </div>
                </details>
              {/each}
            </div>
          {:else}
            <Empty.Root><Empty.Header><Empty.Title>No events</Empty.Title><Empty.Description>No session events are available.</Empty.Description></Empty.Header></Empty.Root>
          {/if}
        </Card.Content>
      </Card.Root>
    </Tabs.Content>
  </Tabs.Root>
{/if}
</section>
