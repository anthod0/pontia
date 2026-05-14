<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, RefreshCw, Send, ShieldAlert, TerminalSquare } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import { formatDateTime, jsonPreview, shortId } from '../components/tasks/format'
  import type { AgentProfileView, InboxDeliveryPolicy, SessionView, WorkspaceView } from '../api/types'
  import { selectCurrentTurnOutput } from './sessions/currentTurnOutput'
  import { isTerminalSession, sessionDisplayTitle, visibleSessionsForFilter, type SessionFilter } from './sessions/sessionList'
  import {
    clientTypeOptionsForProfile,
    defaultHandleForProfile,
    loadAgentProfiles,
    sessionProfileFields,
    agentProfiles,
  } from '../stores/agentProfiles'
  import { loadWorkspaces, workspaces } from '../stores/workspaces'
  import {
    createSession,
    discoverSessionArtifacts,
    interruptSession,
    loadSessionDetail,
    loadSessions,
    restartSession,
    sessionDetail,
    sessionDetailError,
    sessionDetailLoading,
    sessions,
    sessionsError,
    sessionsLoading,
    submitInboxMessage,
    terminateSession,
  } from '../stores/sessions'

  let selectedSessionId = ''
  let sessionFilter: SessionFilter = 'active'
  let actionError: string | null = null
  let actionMessage: string | null = null
  let actionBusy = false

  let createClientType = 'generic'
  let createWorkspaceId = ''
  let createProfileId = ''
  let createHandle = ''
  let createRole = ''
  let createDescription = ''
  let initialInput = ''
  let creating = false

  let inboxInput = ''
  let inboxPolicy: InboxDeliveryPolicy = 'after_idle'
  let submittingInbox = false

  onMount(async () => {
    await Promise.all([loadSessions(), loadWorkspaces(), loadAgentProfiles()])
    if (!selectedSessionId) selectedSessionId = visibleSessionsForFilter($sessions, 'active')[0]?.session_id ?? ''
    if (!createWorkspaceId && $workspaces.length) createWorkspaceId = $workspaces[0].workspace_id
    if (selectedSessionId) await loadSessionDetail(selectedSessionId)
  })

  $: visibleSessions = visibleSessionsForFilter($sessions, sessionFilter)
  $: activeSessionCount = $sessions.filter((session) => !isTerminalSession(session)).length
  $: exitedSessionCount = $sessions.filter((session) => isTerminalSession(session)).length
  $: selectedSession = $sessions.find((session) => session.session_id === selectedSessionId) ?? $sessionDetail?.session ?? null
  $: currentTurnOutput = $sessionDetail ? selectCurrentTurnOutput($sessionDetail.session, $sessionDetail.turns) : null
  $: selectedProfile = $agentProfiles.find((profile) => profile.profile_id === createProfileId) ?? null
  $: selectedWorkspace = $workspaces.find((workspace) => workspace.workspace_id === createWorkspaceId) ?? null
  $: clientTypeOptions = clientTypeOptionsForProfile(selectedProfile)
  $: if (!clientTypeOptions.includes(createClientType)) createClientType = clientTypeOptions[0] ?? createClientType
  $: canCreate = createClientType.trim().length > 0 && createWorkspaceId.trim().length > 0 && !creating
  $: canSubmitInbox = Boolean(selectedSessionId && inboxInput.trim() && !submittingInbox)
  $: normalTurnReason = 'Direct POST /sessions/:id/turns is not exposed by the External API in this build. Use the inbox controls below.'
  $: interruptReason = interruptUnsupportedReason(selectedSession)
  $: restartReason = selectedSession && isTerminalSession(selectedSession) ? 'Terminal sessions cannot be restarted.' : null
  $: terminateReason = selectedSession && isTerminalSession(selectedSession) ? 'Session is already terminal.' : null

  function sessionTitle(session: SessionView): string {
    return sessionDisplayTitle(session)
  }

  function workspaceTitle(workspace: WorkspaceView): string {
    return workspace.name ?? workspace.display_path ?? workspace.workspace_id
  }

  function profileTitle(profile: AgentProfileView): string {
    return `${profile.name} (${profile.profile_id}@${profile.version})`
  }

  function interruptUnsupportedReason(session: SessionView | null): string | null {
    if (!session) return 'Select a session first.'
    if (!session.capabilities?.interrupt) return 'Selected session runtime does not advertise interrupt capability.'
    if (!session.current_turn_id) return 'Selected session has no active turn to interrupt.'
    return null
  }

  async function selectSession(sessionId: string): Promise<void> {
    selectedSessionId = sessionId
    actionError = null
    actionMessage = null
    await loadSessionDetail(sessionId)
  }

  async function refreshAll(): Promise<void> {
    actionError = null
    actionMessage = null
    const loaded = await loadSessions()
    if (!selectedSessionId) selectedSessionId = visibleSessionsForFilter(loaded, 'active')[0]?.session_id ?? ''
    await Promise.all([loadWorkspaces(), loadAgentProfiles()])
    if (selectedSessionId) await loadSessionDetail(selectedSessionId)
  }

  function applyProfileDefaults(): void {
    if (!selectedProfile) return
    createClientType = clientTypeOptionsForProfile(selectedProfile)[0] ?? createClientType
    createHandle = defaultHandleForProfile(selectedProfile)
    createRole = selectedProfile.default_session_role ?? ''
    createDescription = selectedProfile.default_session_description ?? ''
  }

  async function createManualSession(): Promise<void> {
    if (!canCreate) return
    creating = true
    actionError = null
    actionMessage = null
    try {
      const result = await createSession({
        client_type: createClientType.trim(),
        workspace_id: createWorkspaceId,
        handle: createHandle.trim() || null,
        role: createRole.trim() || null,
        description: createDescription.trim() || null,
        ...sessionProfileFields(selectedProfile),
        initial_task: initialInput.trim() ? { input: initialInput.trim(), metadata: { source: 'dashboard_session_console' } } : null,
        metadata: { source: 'dashboard_session_console' },
      })
      selectedSessionId = result.session.session_id
      initialInput = ''
      actionMessage = result.initial_turn ? 'Session created and initial input was queued.' : 'Session created.'
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error)
    } finally {
      creating = false
    }
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
        metadata: { source: 'dashboard_session_console' },
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

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <Badge variant="secondary">Advanced manual console</Badge>
      <h2 class="flex items-center gap-2 text-3xl font-semibold tracking-tight"><TerminalSquare class="size-7" /> Sessions</h2>
      <p class="max-w-3xl text-muted-foreground">Direct operator controls for creating, driving, interrupting, restarting, and terminating sessions. DAG Tasks remain the primary workflow.</p>
    </div>
    <Button variant="outline" onclick={() => void refreshAll()}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  <Alert.Root>
    <ShieldAlert class="size-4" />
    <Alert.Title>External API only</Alert.Title>
    <Alert.Description>This console uses registered workspaces, profiles, session projections, turns, inbox messages, events, and artifacts returned by `/external/v1/*`; it does not infer state from runtime files, tmux, SQLite, or workspace contents.</Alert.Description>
  </Alert.Root>

  {#if $sessionsError || $sessionDetailError || actionError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Session console error</Alert.Title>
      <Alert.Description>{actionError ?? $sessionDetailError ?? $sessionsError}</Alert.Description>
    </Alert.Root>
  {/if}
  {#if actionMessage}
    <Alert.Root>
      <Alert.Title>Action completed</Alert.Title>
      <Alert.Description>{actionMessage}</Alert.Description>
    </Alert.Root>
  {/if}

  <div class="grid gap-4 xl:grid-cols-[22rem_minmax(0,1fr)]">
    <div class="space-y-4">
      <Card.Root>
        <Card.Header>
          <Card.Title>Create manual session</Card.Title>
          <Card.Description>Starts a session from registered workspace/profile data through the External API.</Card.Description>
        </Card.Header>
        <Card.Content class="space-y-3">
          <div class="space-y-2">
            <Label for="session-profile">Profile (optional)</Label>
            <select id="session-profile" bind:value={createProfileId} onchange={applyProfileDefaults} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
              <option value="">No profile</option>
              {#each $agentProfiles as profile}
                <option value={profile.profile_id}>{profileTitle(profile)}</option>
              {/each}
            </select>
          </div>
          <div class="space-y-2">
            <Label for="session-client">Client type</Label>
            <select id="session-client" bind:value={createClientType} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
              {#each clientTypeOptions as clientType}
                <option value={clientType}>{clientType}</option>
              {/each}
            </select>
          </div>
          <div class="space-y-2">
            <Label for="session-workspace">Workspace</Label>
            <select id="session-workspace" bind:value={createWorkspaceId} class="h-9 w-full rounded-md border bg-transparent px-3 text-sm">
              <option value="">Select a registered workspace</option>
              {#each $workspaces as workspace}
                <option value={workspace.workspace_id}>{workspaceTitle(workspace)}</option>
              {/each}
            </select>
            <p class="text-xs text-muted-foreground">{selectedWorkspace?.canonical_path ?? 'Register a workspace first if none are available.'}</p>
          </div>
          <div class="grid gap-3 sm:grid-cols-2">
            <div class="space-y-2"><Label for="session-handle">Handle</Label><Input id="session-handle" bind:value={createHandle} placeholder="@operator" /></div>
            <div class="space-y-2"><Label for="session-role">Role</Label><Input id="session-role" bind:value={createRole} placeholder="manual operator" /></div>
          </div>
          <div class="space-y-2"><Label for="session-description">Description</Label><Input id="session-description" bind:value={createDescription} placeholder="Manual diagnostics session" /></div>
          <div class="space-y-2"><Label for="initial-input">Initial input (optional)</Label><Textarea id="initial-input" bind:value={initialInput} placeholder="Start by inspecting…" /></div>
          <Button class="w-full" onclick={createManualSession} disabled={!canCreate}>{creating ? 'Creating…' : 'Create session'}</Button>
        </Card.Content>
      </Card.Root>

      <Card.Root>
        <Card.Header>
          <Card.Title>Sessions</Card.Title>
          <Card.Description>{activeSessionCount} active · {exitedSessionCount} exited/error · {$sessions.length} total.</Card.Description>
        </Card.Header>
        <Card.Content class="space-y-3">
          <Tabs.Root bind:value={sessionFilter} class="gap-3">
            <Tabs.List class="grid w-full grid-cols-3">
              <Tabs.Trigger value="active">Active ({activeSessionCount})</Tabs.Trigger>
              <Tabs.Trigger value="exited">Exited ({exitedSessionCount})</Tabs.Trigger>
              <Tabs.Trigger value="all">All ({$sessions.length})</Tabs.Trigger>
            </Tabs.List>
          </Tabs.Root>

          {#if $sessionsLoading}
            <div class="space-y-2"><Skeleton class="h-16 w-full" /><Skeleton class="h-16 w-full" /><Skeleton class="h-16 w-full" /></div>
          {:else if !$sessions.length}
            <Empty.Root><Empty.Header><Empty.Title>No sessions</Empty.Title><Empty.Description>Create a manual session or start a DAG task.</Empty.Description></Empty.Header></Empty.Root>
          {:else if !visibleSessions.length}
            <Empty.Root><Empty.Header><Empty.Title>No {sessionFilter} sessions</Empty.Title><Empty.Description>Switch tabs to inspect other session states.</Empty.Description></Empty.Header></Empty.Root>
          {:else}
            {#each visibleSessions as session}
              <button class="w-full rounded-lg border p-3 text-left text-sm transition hover:bg-muted {selectedSessionId === session.session_id ? 'border-primary bg-muted' : ''}" onclick={() => void selectSession(session.session_id)}>
                <div class="flex items-center justify-between gap-2"><span class="font-medium">{sessionTitle(session)}</span><Badge variant="secondary">{session.state}</Badge></div>
                <div class="mt-1 truncate text-xs text-muted-foreground">{session.client_type} · {session.workspace_id ?? 'no workspace'}</div>
                <div class="mt-2 text-xs text-muted-foreground">Updated {formatDateTime(session.updated_at)}</div>
              </button>
            {/each}
          {/if}
        </Card.Content>
      </Card.Root>
    </div>

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
          <Card.Header><Card.Title>Session events</Card.Title><Card.Description>{$sessionDetail.events.length} events with payload previews.</Card.Description></Card.Header>
          <Card.Content>
            {#if $sessionDetail.events.length}
              <div class="space-y-3">
                {#each $sessionDetail.events.slice(0, 50) as event}
                  <div class="rounded-lg border p-3 text-sm">
                    <div class="flex flex-wrap items-center justify-between gap-2"><span class="font-medium">{event.type}</span><span class="text-xs text-muted-foreground">{formatDateTime(event.time)}</span></div>
                    <pre class="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">{JSON.stringify(event.payload, null, 2)}</pre>
                  </div>
                {/each}
              </div>
            {:else}
              <Empty.Root><Empty.Header><Empty.Title>No events</Empty.Title><Empty.Description>No session events are available.</Empty.Description></Empty.Header></Empty.Root>
            {/if}
          </Card.Content>
        </Card.Root>
      </div>
    {/if}
  </div>
</section>
