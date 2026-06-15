<script lang="ts">
  import { onMount } from 'svelte'
  import { Bot, CircleAlert, CopyPlus, Pencil, Plus, RefreshCw, Trash2 } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import {
    createAgentProfile,
    createAgentProfileVersion,
    deleteAgentProfile,
    deleteAgentProfileVersion,
    listAgentProfileVersions,
    updateAgentProfileVersion,
  } from '../api/client'
  import type { AgentProfileView } from '../api/types'
  import { formatDateTime } from '../components/tasks/format'
  import { agentProfiles, agentProfilesError, agentProfilesLoading, loadAgentProfiles } from '../stores/agentProfiles'
  import {
    buildAgentProfileInput,
    createAgentProfileDraft,
    createAgentProfileDraftFromProfile,
    type AgentProfileDraft,
    type AgentProfileDraftErrors,
  } from './agentProfiles/form'

  type FormMode = 'create' | 'edit' | 'new-version'

  let selectedProfileId = ''
  let selectedVersion = ''
  let includeArchivedProfiles = false
  let includeArchivedVersions = false
  let versions: AgentProfileView[] = []
  let versionsProfileId = ''
  let versionsLoading = false
  let versionsError: string | null = null
  let formMode: FormMode | null = null
  let draft: AgentProfileDraft = createAgentProfileDraft()
  let draftErrors: AgentProfileDraftErrors = {}
  let mutationError: string | null = null
  let mutationSuccess: string | null = null
  let saving = false
  let archiveDialogOpen = false
  let pendingArchive: 'profile' | 'version' | null = null
  let pageAbortController: AbortController | null = null

  onMount(() => {
    pageAbortController = new AbortController()

    void (async () => {
      await refreshProfiles({ signal: pageAbortController?.signal })
      if (!pageAbortController?.signal.aborted && !selectedProfileId && $agentProfiles.length) selectedProfileId = $agentProfiles[0].profile_id
    })()

    return () => {
      pageAbortController?.abort()
      pageAbortController = null
    }
  })

  $: sortedProfiles = [...$agentProfiles].sort((a, b) => a.name.localeCompare(b.name))
  $: selectedProfile = sortedProfiles.find((profile) => profile.profile_id === selectedProfileId) ?? sortedProfiles[0] ?? null
  $: sortedVersions = [...versions].sort((a, b) => b.updated_at.localeCompare(a.updated_at))
  $: selectedVersionProfile = sortedVersions.find((profile) => profile.version === selectedVersion) ?? sortedVersions[0] ?? selectedProfile
  $: if (selectedProfile && selectedProfile.profile_id !== versionsProfileId && !versionsLoading) void loadVersions(selectedProfile.profile_id, { signal: pageAbortController?.signal })
  $: profileRows = selectedVersionProfile ? [
    { label: 'Artifact contract', value: JSON.stringify(selectedVersionProfile.artifact_contract ?? {}, null, 2) },
    { label: 'Execution policy', value: JSON.stringify(selectedVersionProfile.default_execution_policy ?? {}, null, 2) },
    { label: 'Review policy', value: JSON.stringify(selectedVersionProfile.default_review_policy ?? {}, null, 2) },
    { label: 'Metadata', value: JSON.stringify(selectedVersionProfile.metadata ?? {}, null, 2) },
    { label: 'Expected output schema', value: selectedVersionProfile.expected_output_schema ?? 'Not configured' },
  ] : []
  $: builtinSelected = Boolean(selectedVersionProfile?.metadata?.builtin)
  $: archiveDialogTitle = pendingArchive === 'profile' ? 'Archive profile?' : 'Archive profile version?'
  $: archiveDialogDescription = pendingArchive === 'profile'
    ? selectedProfile ? `Archive profile ${selectedProfile.profile_id} and all active versions?` : ''
    : selectedVersionProfile ? `Archive version ${selectedVersionProfile.profile_id}@${selectedVersionProfile.version}?` : ''
  $: if (!archiveDialogOpen && pendingArchive && !saving) pendingArchive = null

  async function refreshProfiles(options: { signal?: AbortSignal } = {}): Promise<void> {
    await loadAgentProfiles(includeArchivedProfiles, options)
  }

  async function refreshAll(): Promise<void> {
    await refreshProfiles()
    if (selectedProfileId) await loadVersions(selectedProfileId)
  }

  async function loadVersions(profileId: string, options: { signal?: AbortSignal } = {}): Promise<void> {
    versionsLoading = true
    versionsError = null
    versionsProfileId = profileId
    try {
      versions = await listAgentProfileVersions(profileId, includeArchivedVersions, options)
      selectedVersion = versions.find((profile) => profile.version === selectedVersion)?.version ?? versions[0]?.version ?? ''
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') return
      versions = []
      versionsError = error instanceof Error ? error.message : String(error)
    } finally {
      versionsLoading = false
    }
  }

  function selectProfile(profileId: string): void {
    selectedProfileId = profileId
    selectedVersion = ''
    formMode = null
    mutationError = null
    mutationSuccess = null
  }

  function startCreate(): void {
    formMode = 'create'
    draft = createAgentProfileDraft()
    draftErrors = {}
    mutationError = null
    mutationSuccess = null
  }

  function startEdit(profile: AgentProfileView): void {
    formMode = 'edit'
    draft = createAgentProfileDraftFromProfile(profile)
    draftErrors = {}
    mutationError = null
    mutationSuccess = null
  }

  function startNewVersion(profile: AgentProfileView): void {
    formMode = 'new-version'
    draft = createAgentProfileDraftFromProfile(profile, { clearVersion: true })
    draftErrors = {}
    mutationError = null
    mutationSuccess = null
  }

  function cancelForm(): void {
    formMode = null
    draftErrors = {}
    mutationError = null
  }

  async function submitForm(): Promise<void> {
    const result = buildAgentProfileInput(draft)
    draftErrors = result.ok ? {} : result.errors
    mutationError = null
    mutationSuccess = null
    if (!result.ok || !formMode) return

    saving = true
    try {
      let saved: AgentProfileView
      if (formMode === 'create') {
        saved = await createAgentProfile(result.input)
      } else if (formMode === 'new-version') {
        saved = await createAgentProfileVersion(result.input.profile_id, result.input)
      } else {
        saved = await updateAgentProfileVersion(result.input.profile_id, result.input.version, result.input)
      }
      selectedProfileId = saved.profile_id
      selectedVersion = saved.version
      formMode = null
      mutationSuccess = `Saved ${saved.profile_id}@${saved.version}.`
      await refreshAll()
    } catch (error) {
      mutationError = error instanceof Error ? error.message : String(error)
    } finally {
      saving = false
    }
  }

  function requestArchiveSelectedProfile(): void {
    if (!selectedProfile) return
    pendingArchive = 'profile'
    archiveDialogOpen = true
  }

  function requestArchiveSelectedVersion(): void {
    if (!selectedVersionProfile) return
    pendingArchive = 'version'
    archiveDialogOpen = true
  }

  async function archiveSelectedProfile(): Promise<void> {
    if (!selectedProfile) return
    mutationError = null
    mutationSuccess = null
    saving = true
    try {
      const result = await deleteAgentProfile(selectedProfile.profile_id)
      mutationSuccess = `Archived ${result.archived_versions} version(s) for ${result.profile_id}.`
      selectedProfileId = ''
      selectedVersion = ''
      versions = []
      versionsProfileId = ''
      await refreshProfiles()
      if ($agentProfiles.length) selectedProfileId = $agentProfiles[0].profile_id
    } catch (error) {
      mutationError = error instanceof Error ? error.message : String(error)
    } finally {
      saving = false
    }
  }

  async function archiveSelectedVersion(): Promise<void> {
    if (!selectedVersionProfile) return
    mutationError = null
    mutationSuccess = null
    saving = true
    try {
      const archived = await deleteAgentProfileVersion(selectedVersionProfile.profile_id, selectedVersionProfile.version)
      mutationSuccess = `Archived ${archived.profile_id}@${archived.version}.`
      await refreshAll()
    } catch (error) {
      mutationError = error instanceof Error ? error.message : String(error)
    } finally {
      saving = false
    }
  }

  async function confirmPendingArchive(): Promise<void> {
    const archiveTarget = pendingArchive
    archiveDialogOpen = false
    pendingArchive = null
    if (archiveTarget === 'profile') {
      await archiveSelectedProfile()
    } else if (archiveTarget === 'version') {
      await archiveSelectedVersion()
    }
  }

  function templateSummary(value: string | null): string {
    if (!value?.trim()) return 'Not configured'
    return value.length > 180 ? `${value.slice(0, 180)}…` : value
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="text-3xl font-semibold tracking-tight">Agent Profiles</h2>
      <p class="max-w-3xl text-muted-foreground">Manage execution profiles, versions, client support, prompts, and policy metadata from the External API.</p>
    </div>
    <div class="flex flex-wrap gap-2">
      <Button onclick={startCreate}><Plus class="size-4" /> Create profile</Button>
      <Button variant="outline" onclick={() => void refreshAll()}><RefreshCw class="size-4" /> Refresh</Button>
    </div>
  </div>

  {#if $agentProfilesError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Unable to load profiles</Alert.Title>
      <Alert.Description>{$agentProfilesError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if versionsError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Unable to load versions</Alert.Title>
      <Alert.Description>{versionsError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if mutationError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Profile operation failed</Alert.Title>
      <Alert.Description>{mutationError}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if mutationSuccess}
    <Alert.Root>
      <Alert.Title>Success</Alert.Title>
      <Alert.Description>{mutationSuccess}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if formMode}
    <Card.Root>
      <Card.Header>
        <Card.Title>{formMode === 'create' ? 'Create profile' : formMode === 'new-version' ? 'Create profile version' : 'Edit profile version'}</Card.Title>
        <Card.Description>Required fields are profile ID, version, and name. JSON fields must be objects.</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-5">
        <div class="grid gap-4 md:grid-cols-3">
          <div class="space-y-2">
            <Label for="profile-id">Profile ID</Label>
            <Input id="profile-id" bind:value={draft.profile_id} disabled={formMode !== 'create'} />
            {#if draftErrors.profile_id}<p class="text-xs text-destructive">{draftErrors.profile_id}</p>{/if}
          </div>
          <div class="space-y-2">
            <Label for="profile-version">Version</Label>
            <Input id="profile-version" bind:value={draft.version} disabled={formMode === 'edit'} />
            {#if draftErrors.version}<p class="text-xs text-destructive">{draftErrors.version}</p>{/if}
          </div>
          <div class="space-y-2">
            <Label for="profile-name">Name</Label>
            <Input id="profile-name" bind:value={draft.name} />
            {#if draftErrors.name}<p class="text-xs text-destructive">{draftErrors.name}</p>{/if}
          </div>
        </div>

        <div class="grid gap-4 md:grid-cols-2">
          <div class="space-y-2">
            <Label for="profile-description">Description</Label>
            <Textarea id="profile-description" bind:value={draft.description} rows={3} />
          </div>
          <div class="space-y-2">
            <Label for="profile-clients">Supported client types</Label>
            <Input id="profile-clients" bind:value={draft.supported_client_types_text} placeholder="pi, claude_code" />
            {#if draftErrors.supported_client_types_text}<p class="text-xs text-destructive">{draftErrors.supported_client_types_text}</p>{/if}
          </div>
          <div class="space-y-2">
            <Label for="profile-agent-kind">Agent kind</Label>
            <select id="profile-agent-kind" bind:value={draft.agent_kind} class="h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-xs">
              <option value="executor">executor</option>
              <option value="planner">planner</option>
            </select>
          </div>
          <div class="space-y-2">
            <Label for="profile-role">Default session role</Label>
            <Input id="profile-role" bind:value={draft.default_session_role} />
          </div>
          <div class="space-y-2">
            <Label for="profile-handle">Handle prefix</Label>
            <Input id="profile-handle" bind:value={draft.handle_prefix} />
          </div>
          <div class="space-y-2 md:col-span-2">
            <Label for="profile-session-description">Default session description</Label>
            <Input id="profile-session-description" bind:value={draft.default_session_description} />
          </div>
        </div>

        <div class="grid gap-4 xl:grid-cols-2">
          <div class="space-y-2">
            <Label for="profile-system-prompt">System prompt template</Label>
            <Textarea id="profile-system-prompt" bind:value={draft.system_prompt_template} rows={8} class="font-mono text-xs" />
          </div>
          <div class="space-y-2">
            <Label for="profile-turn-prompt">Turn prompt template</Label>
            <Textarea id="profile-turn-prompt" bind:value={draft.turn_prompt_template} rows={8} class="font-mono text-xs" />
          </div>
          <div class="space-y-2 xl:col-span-2">
            <Label for="profile-output-schema">Expected output schema</Label>
            <Textarea id="profile-output-schema" bind:value={draft.expected_output_schema} rows={4} class="font-mono text-xs" />
          </div>
        </div>

        <div class="grid gap-4 xl:grid-cols-2">
          <div class="space-y-2">
            <Label for="artifact_contract_text">Artifact contract</Label>
            <Textarea id="artifact_contract_text" bind:value={draft.artifact_contract_text} rows={7} class="font-mono text-xs" />
            {#if draftErrors.artifact_contract_text}<p class="text-xs text-destructive">{draftErrors.artifact_contract_text}</p>{/if}
          </div>
          <div class="space-y-2">
            <Label for="default_execution_policy_text">Default execution policy</Label>
            <Textarea id="default_execution_policy_text" bind:value={draft.default_execution_policy_text} rows={7} class="font-mono text-xs" />
            {#if draftErrors.default_execution_policy_text}<p class="text-xs text-destructive">{draftErrors.default_execution_policy_text}</p>{/if}
          </div>
          <div class="space-y-2">
            <Label for="default_review_policy_text">Default review policy</Label>
            <Textarea id="default_review_policy_text" bind:value={draft.default_review_policy_text} rows={7} class="font-mono text-xs" />
            {#if draftErrors.default_review_policy_text}<p class="text-xs text-destructive">{draftErrors.default_review_policy_text}</p>{/if}
          </div>
          <div class="space-y-2">
            <Label for="metadata_text">Metadata</Label>
            <Textarea id="metadata_text" bind:value={draft.metadata_text} rows={7} class="font-mono text-xs" />
            {#if draftErrors.metadata_text}<p class="text-xs text-destructive">{draftErrors.metadata_text}</p>{/if}
          </div>
        </div>
      </Card.Content>
      <Card.Footer class="flex flex-wrap justify-end gap-2">
        <Button variant="outline" onclick={cancelForm} disabled={saving}>Cancel</Button>
        <Button onclick={() => void submitForm()} disabled={saving}>{saving ? 'Saving…' : 'Save profile'}</Button>
      </Card.Footer>
    </Card.Root>
  {/if}

  <div class="flex flex-wrap gap-4 text-sm text-muted-foreground">
    <label class="flex items-center gap-2"><input type="checkbox" bind:checked={includeArchivedProfiles} onchange={() => void refreshProfiles()} /> Show archived profiles</label>
    <label class="flex items-center gap-2"><input type="checkbox" bind:checked={includeArchivedVersions} onchange={() => selectedProfileId && void loadVersions(selectedProfileId)} /> Show archived versions</label>
  </div>

  {#if $agentProfilesLoading}
    <div class="grid gap-4 lg:grid-cols-[22rem_1fr]"><Skeleton class="h-96 w-full" /><Skeleton class="h-96 w-full" /></div>
  {:else if !sortedProfiles.length}
    <Empty.Root><Empty.Header><Empty.Title>No agent profiles</Empty.Title><Empty.Description>No execution profiles are available from /external/v1/agent-profiles.</Empty.Description></Empty.Header></Empty.Root>
  {:else}
    <div class="grid gap-4 lg:grid-cols-[22rem_minmax(0,1fr)]">
      <Card.Root>
        <Card.Header><Card.Title class="flex items-center gap-2"><Bot class="size-5" /> Profiles</Card.Title><Card.Description>{sortedProfiles.length} configured profiles.</Card.Description></Card.Header>
        <Card.Content class="space-y-2">
          {#each sortedProfiles as profile}
            <button
              class="w-full rounded-lg border p-3 text-left transition hover:bg-muted {selectedProfile?.profile_id === profile.profile_id ? 'border-primary bg-muted' : ''}"
              onclick={() => selectProfile(profile.profile_id)}
            >
              <div class="flex items-start justify-between gap-2">
                <div>
                  <div class="font-medium">{profile.name}</div>
                  <div class="truncate text-xs text-muted-foreground">{profile.profile_id} · v{profile.version}</div>
                </div>
                {#if !profile.active}<Badge variant="outline">Archived</Badge>{/if}
              </div>
              <div class="mt-2 flex flex-wrap gap-1">
                <Badge>{profile.agent_kind}</Badge>
                {#each profile.supported_client_types as client}
                  <Badge variant="secondary">{client}</Badge>
                {/each}
              </div>
            </button>
          {/each}
        </Card.Content>
      </Card.Root>

      {#if selectedVersionProfile}
        <div class="space-y-4">
          <Card.Root>
            <Card.Header>
              <div class="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
                <div>
                  <Card.Title>{selectedVersionProfile.name}</Card.Title>
                  <Card.Description>{selectedVersionProfile.description ?? 'No description provided.'}</Card.Description>
                </div>
                <div class="flex flex-wrap gap-2">
                  <Button variant="outline" onclick={() => startNewVersion(selectedVersionProfile)} disabled={saving || builtinSelected}><CopyPlus class="size-4" /> New version</Button>
                  <Button variant="outline" onclick={() => startEdit(selectedVersionProfile)} disabled={saving || builtinSelected || !selectedVersionProfile.active}><Pencil class="size-4" /> Edit</Button>
                  <Button variant="outline" onclick={requestArchiveSelectedVersion} disabled={saving || builtinSelected || !selectedVersionProfile.active}><Trash2 class="size-4" /> Delete version</Button>
                  <Button variant="destructive" onclick={requestArchiveSelectedProfile} disabled={saving || builtinSelected}><Trash2 class="size-4" /> Delete profile</Button>
                </div>
              </div>
            </Card.Header>
            <Card.Content class="space-y-4">
              <div class="grid gap-3 text-sm md:grid-cols-2 xl:grid-cols-3">
                {#each [
                  ['Profile ID', selectedVersionProfile.profile_id],
                  ['Version', selectedVersionProfile.version],
                  ['Agent kind', selectedVersionProfile.agent_kind],
                  ['Session role', selectedVersionProfile.default_session_role ?? '—'],
                  ['Handle prefix', selectedVersionProfile.handle_prefix ?? '—'],
                  ['State', selectedVersionProfile.active ? 'Active' : 'Archived'],
                  ['Updated', formatDateTime(selectedVersionProfile.updated_at)],
                ] as [label, value]}
                  <div class="rounded-lg border p-3">
                    <div class="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
                    <div class="mt-1 break-words font-medium">{value}</div>
                  </div>
                {/each}
              </div>
            </Card.Content>
          </Card.Root>

          <Card.Root>
            <Card.Header><Card.Title>Versions</Card.Title><Card.Description>{versionsLoading ? 'Loading versions…' : `${sortedVersions.length} version(s) for this profile.`}</Card.Description></Card.Header>
            <Card.Content class="space-y-2">
              {#each sortedVersions as version}
                <button class="w-full rounded-lg border p-3 text-left transition hover:bg-muted {selectedVersionProfile?.version === version.version ? 'border-primary bg-muted' : ''}" onclick={() => selectedVersion = version.version}>
                  <div class="flex items-center justify-between gap-2">
                    <span class="font-medium">v{version.version}</span>
                    <span class="text-xs text-muted-foreground">{formatDateTime(version.updated_at)}</span>
                  </div>
                  <div class="mt-2 flex flex-wrap gap-1">
                    {#if version.active}<Badge>Active</Badge>{:else}<Badge variant="outline">Archived</Badge>{/if}
                    {#if version.metadata?.builtin}<Badge variant="secondary">Builtin</Badge>{/if}
                  </div>
                </button>
              {:else}
                <p class="text-sm text-muted-foreground">No versions returned for this profile.</p>
              {/each}
            </Card.Content>
          </Card.Root>

          <Card.Root>
            <Card.Header><Card.Title>Client support</Card.Title><Card.Description>Client-specific behavior stays behind adapter/runtime capability boundaries.</Card.Description></Card.Header>
            <Card.Content class="flex flex-wrap gap-2">
              {#each selectedVersionProfile.supported_client_types as client}
                <Badge>{client}</Badge>
              {:else}
                <span class="text-sm text-muted-foreground">No supported client types listed.</span>
              {/each}
            </Card.Content>
          </Card.Root>

          <Card.Root>
            <Card.Header><Card.Title>Prompt templates</Card.Title><Card.Description>Selected profile version detail.</Card.Description></Card.Header>
            <Card.Content class="grid gap-4 xl:grid-cols-2">
              <div class="space-y-2"><div class="text-sm font-medium">System prompt</div><pre class="max-h-72 overflow-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{templateSummary(selectedVersionProfile.system_prompt_template)}</pre></div>
              <div class="space-y-2"><div class="text-sm font-medium">Turn prompt</div><pre class="max-h-72 overflow-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{templateSummary(selectedVersionProfile.turn_prompt_template)}</pre></div>
            </Card.Content>
          </Card.Root>

          <Card.Root>
            <Card.Header><Card.Title>Policies and metadata</Card.Title><Card.Description>External API projection values.</Card.Description></Card.Header>
            <Card.Content>
              <div class="overflow-x-auto">
                <Table.Root>
                  <Table.Header><Table.Row><Table.Head>Field</Table.Head><Table.Head>Value</Table.Head></Table.Row></Table.Header>
                  <Table.Body>
                    {#each profileRows as row}
                      <Table.Row><Table.Cell class="font-medium">{row.label}</Table.Cell><Table.Cell><pre class="max-h-44 overflow-auto whitespace-pre-wrap text-xs">{row.value}</pre></Table.Cell></Table.Row>
                    {/each}
                  </Table.Body>
                </Table.Root>
              </div>
            </Card.Content>
          </Card.Root>
        </div>
      {/if}
    </div>
  {/if}
</section>

<AlertDialog.Root bind:open={archiveDialogOpen}>
  {#if pendingArchive}
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>{archiveDialogTitle}</AlertDialog.Title>
        <AlertDialog.Description>{archiveDialogDescription}</AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel disabled={saving}>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action variant="destructive" onclick={() => void confirmPendingArchive()} disabled={saving}>
          {saving ? 'Archiving…' : 'Archive'}
        </AlertDialog.Action>
      </AlertDialog.Footer>
    </AlertDialog.Content>
  {/if}
</AlertDialog.Root>
