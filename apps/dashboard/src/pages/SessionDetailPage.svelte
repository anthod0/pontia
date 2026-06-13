<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, MessageCircle, RefreshCw, Send, ShieldAlert, TerminalSquare } from '@lucide/svelte'
  import { getPathParams, navigate } from 'svelte-mini-router'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import { formatDateTime, jsonPreview, shortId } from '../components/tasks/format'
  import { contextUsageRatio, contextUsageSummary } from '$lib/contextUsage'
  import type { ContextUsageView, InboxDeliveryPolicy, SessionView } from '../api/types'
  import { selectCurrentTurnOutput } from './sessions/currentTurnOutput'
  import { sessionEventDetailRows, sessionEventSummary, sessionEventTurnLabel } from './sessions/sessionEvents'
  import { isTerminalSession, sessionDisplayTitle } from './sessions/sessionList'
  import {
    discoverSessionArtifacts,
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
  $: canSubmitInbox = Boolean(selectedSessionId && inboxInput.trim() && !submittingInbox)
  $: normalTurnReason = 'Direct POST /sessions/:id/turns is not exposed by the External API in this build. Use the inbox controls below.'
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

  function interruptUnsupportedReason(session: SessionView | null): string | null {
    if (!session) return 'Select a session first.'
    if (!session.capabilities?.interrupt) return 'Selected session runtime does not advertise interrupt capability.'
    if (!session.current_turn_id) return 'Selected session has no active turn to interrupt.'
    return null
  }

  function requestedSessionIdFromLocation(): string {
    return getPathParams().sessionId ?? new URLSearchParams(window.location.search).get('session') ?? ''
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

  async function runControl(action: 'interrupt' | 'restart' | 'terminate' | 'discover'): Promise<void> {
    if (!selectedSessionId) return
    actionBusy = true
    actionError = null
    actionMessage = null
    try {
      if (action === 'interrupt') await interruptSession(selectedSessionId)
      if (action === 'restart') await restartSession(selectedSessionId)
      if (action === 'terminate') await terminateSession(selectedSessionId)
      if (action === 'discover') await discoverSessionArtifacts(selectedSessionId)
      actionMessage = action === 'discover' ? 'Artifact discovery refreshed.' : `Session ${action} request accepted.`
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
      <p class="max-w-3xl text-muted-foreground">Inspect and operate on one session. Use the sessions page for list browsing.</p>
    </div>
    <div class="flex gap-2">
      <Button variant="outline" onclick={() => navigate('/sessions')}>Back to Sessions</Button>
      <Button variant="outline" onclick={openSelectedSessionChat}><MessageCircle class="size-4" /> Open Chat</Button>
      <Button variant="outline" onclick={() => void refreshAll()}><RefreshCw class="size-4" /> Refresh</Button>
    </div>
  </div>

  <Alert.Root>
    <ShieldAlert class="size-4" />
    <Alert.Title>External API only</Alert.Title>
    <Alert.Description>This detail view uses session projections, turns, inbox messages, events, and artifacts returned by `/external/v1/*`; it does not infer state from runtime files, tmux, SQLite, or workspace contents.</Alert.Description>
  </Alert.Root>

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
  <div class="space-y-4">
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
            ['Current turn', $sessionDetail.session.current_turn_id ?? '—'],
          ] as [label, value]}
            <div class="rounded-lg border p-3"><div class="text-xs uppercase tracking-wide text-muted-foreground">{label}</div><div class="mt-1 break-words font-medium">{value}</div></div>
          {/each}
        </div>
        <div class="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" disabled={actionBusy || Boolean(interruptReason)} title={interruptReason ?? 'Interrupt current turn'} onclick={() => void runControl('interrupt')}>Interrupt</Button>
          <Button size="sm" variant="outline" disabled={actionBusy || Boolean(restartReason)} title={restartReason ?? 'Restart session'} onclick={() => void runControl('restart')}>Restart</Button>
          <Button size="sm" variant="destructive" disabled={actionBusy || Boolean(terminateReason)} title={terminateReason ?? 'Terminate session'} onclick={() => void runControl('terminate')}>Terminate/exit</Button>
          <Button size="sm" variant="outline" disabled={actionBusy} onclick={() => void runControl('discover')}>Discover artifacts</Button>
        </div>
        {#if interruptReason || restartReason || terminateReason}
          <p class="text-xs text-muted-foreground">Unsupported/degraded controls: {interruptReason ?? restartReason ?? terminateReason}</p>
        {/if}
      </Card.Content>
    </Card.Root>

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
              {#if usage.model}<Badge variant="secondary">{usage.model}</Badge>{/if}
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
      <Card.Header>
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div>
            <Card.Title>{currentTurnOutput?.title ?? 'Current turn output'}</Card.Title>
            <Card.Description>
              {#if currentTurnOutput}
                Turn {shortId(currentTurnOutput.turn.turn_id)} · {currentTurnOutput.turn.state} · started {currentTurnOutput.turn.started_at ? formatDateTime(currentTurnOutput.turn.started_at) : '—'}{currentTurnOutput.turn.completed_at ? ` · completed ${formatDateTime(currentTurnOutput.turn.completed_at)}` : ''}
              {:else}
                No turns are available for this session yet.
              {/if}
            </Card.Description>
          </div>
          {#if currentTurnOutput}<Badge variant="secondary">{currentTurnOutput.turn.state}</Badge>{/if}
        </div>
      </Card.Header>
      <Card.Content class="space-y-3">
        {#if currentTurnOutput}
          <div class="rounded-lg border bg-muted/40 p-3 text-sm">
            <div class="mb-1 text-xs uppercase tracking-wide text-muted-foreground">Input</div>
            <div class="whitespace-pre-wrap">{currentTurnOutput.turn.input?.summary ?? jsonPreview(currentTurnOutput.turn.input)}</div>
          </div>
          <div class="rounded-lg border p-3">
            <div class="mb-2 text-xs uppercase tracking-wide text-muted-foreground">Output</div>
            {#if currentTurnOutput.outputSummary}
              <pre class="max-h-[28rem] overflow-auto whitespace-pre-wrap text-sm leading-relaxed">{currentTurnOutput.outputSummary}</pre>
            {:else if currentTurnOutput.turn.failure}
              <pre class="max-h-[28rem] overflow-auto whitespace-pre-wrap text-sm text-destructive">{jsonPreview(currentTurnOutput.turn.failure)}</pre>
            {:else if currentTurnOutput.turn.state === 'running' || currentTurnOutput.turn.state === 'queued'}
              <div class="rounded border border-dashed p-4 text-sm text-muted-foreground">Waiting for output…</div>
            {:else}
              <div class="rounded border border-dashed p-4 text-sm text-muted-foreground">No output summary was reported for this turn.</div>
            {/if}
          </div>
          {#if currentTurnOutput.turn.output?.artifact_ids?.length}
            <p class="text-xs text-muted-foreground">Artifacts: {currentTurnOutput.turn.output.artifact_ids.map(shortId).join(', ')}</p>
          {/if}
        {:else}
          <Empty.Root><Empty.Header><Empty.Title>No current turn output</Empty.Title><Empty.Description>Create or drive a turn to see its output here.</Empty.Description></Empty.Header></Empty.Root>
        {/if}
      </Card.Content>
    </Card.Root>

    <div class="grid gap-4 lg:grid-cols-2">
      <Card.Root>
        <Card.Header><Card.Title>Submit input</Card.Title><Card.Description>Normal turn submission is shown explicitly as unsupported when the API does not expose it.</Card.Description></Card.Header>
        <Card.Content class="space-y-3">
          <div class="rounded-lg border border-dashed p-3 text-sm text-muted-foreground">{normalTurnReason}</div>
          <div class="space-y-2"><Label for="inbox-input">Inbox message</Label><Textarea id="inbox-input" bind:value={inboxInput} placeholder="Send follow-up instructions…" /></div>
          <div class="space-y-2">
            <Label for="inbox-policy">Delivery policy</Label>
            <select id="inbox-policy" bind:value={inboxPolicy} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
              <option value="after_idle">after_idle</option>
              <option value="interrupt_now">interrupt_now</option>
            </select>
            {#if inboxPolicy === 'interrupt_now' && !$sessionDetail.session.capabilities?.interrupt}
              <p class="text-xs text-muted-foreground">This runtime does not advertise interrupt capability; the External API may reject or mark the message failed.</p>
            {/if}
          </div>
          <Button onclick={submitInbox} disabled={!canSubmitInbox}><Send class="size-4" /> {submittingInbox ? 'Submitting…' : 'Submit inbox message'}</Button>
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header><Card.Title>Capabilities</Card.Title><Card.Description>Advertised runtime behavior; unsupported actions are not faked.</Card.Description></Card.Header>
        <Card.Content><pre class="max-h-72 overflow-auto whitespace-pre-wrap rounded bg-muted p-3 text-xs">{JSON.stringify($sessionDetail.session.capabilities, null, 2)}</pre></Card.Content>
      </Card.Root>
    </div>


    <Card.Root>
      <Card.Header><Card.Title>Turns</Card.Title><Card.Description>{$sessionDetail.turns.length} turns with output and artifact references.</Card.Description></Card.Header>
      <Card.Content>
        {#if $sessionDetail.turns.length}
          <div class="overflow-x-auto">
            <Table.Root>
              <Table.Header><Table.Row><Table.Head>Turn</Table.Head><Table.Head>State</Table.Head><Table.Head>Input</Table.Head><Table.Head>Output</Table.Head><Table.Head>Artifacts</Table.Head><Table.Head>Completed</Table.Head></Table.Row></Table.Header>
              <Table.Body>
                {#each $sessionDetail.turns as turn}
                  <Table.Row>
                    <Table.Cell class="font-medium">{shortId(turn.turn_id)}</Table.Cell>
                    <Table.Cell><Badge variant="secondary">{turn.state}</Badge></Table.Cell>
                    <Table.Cell class="max-w-xs truncate">{turn.input?.summary ?? jsonPreview(turn.input)}</Table.Cell>
                    <Table.Cell class="max-w-xs truncate">{turn.output?.summary ?? (turn.failure ? jsonPreview(turn.failure) : '—')}</Table.Cell>
                    <Table.Cell>{turn.output?.artifact_ids?.length ? turn.output.artifact_ids.map(shortId).join(', ') : '—'}</Table.Cell>
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

    <div class="grid gap-4 xl:grid-cols-2">
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

      <Card.Root>
        <Card.Header><Card.Title>Artifacts / output refs</Card.Title><Card.Description>{$sessionDetail.artifacts.length} artifacts discovered through the External API.</Card.Description></Card.Header>
        <Card.Content class="space-y-3">
          {#if $sessionDetail.artifacts.length}
            {#each $sessionDetail.artifacts as artifact}
              <div class="rounded-lg border p-3 text-sm">
                <div class="flex flex-wrap items-center justify-between gap-2"><span class="font-medium">{artifact.name}</span><Badge variant="secondary">{artifact.kind}</Badge></div>
                <div class="mt-1 text-xs text-muted-foreground">{shortId(artifact.artifact_id)} · turn {shortId(artifact.turn_id)} · {formatDateTime(artifact.created_at)}</div>
                {#if artifact.preview}<p class="mt-2 line-clamp-3 text-xs text-muted-foreground">{artifact.preview}</p>{/if}
              </div>
            {/each}
          {:else}
            <Empty.Root><Empty.Header><Empty.Title>No artifacts</Empty.Title><Empty.Description>No session artifacts have been discovered yet.</Empty.Description></Empty.Header></Empty.Root>
          {/if}
        </Card.Content>
      </Card.Root>
    </div>

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
  </div>
{/if}
</section>
