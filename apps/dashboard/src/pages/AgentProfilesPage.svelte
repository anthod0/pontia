<script lang="ts">
  import { onMount } from 'svelte'
  import { Bot, CircleAlert, RefreshCw } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Table from '$lib/components/ui/table/index.js'
  import { formatDateTime } from '../components/tasks/format'
  import { agentProfiles, agentProfilesError, agentProfilesLoading, loadAgentProfiles } from '../stores/agentProfiles'

  let selectedProfileId = ''

  onMount(async () => {
    await loadAgentProfiles()
    if (!selectedProfileId && $agentProfiles.length) selectedProfileId = $agentProfiles[0].profile_id
  })

  $: sortedProfiles = [...$agentProfiles].sort((a, b) => a.name.localeCompare(b.name))
  $: selectedProfile = sortedProfiles.find((profile) => profile.profile_id === selectedProfileId) ?? sortedProfiles[0] ?? null

  $: profileRows = selectedProfile ? [
    { label: 'Artifact contract', value: JSON.stringify(selectedProfile.artifact_contract ?? {}, null, 2) },
    { label: 'Execution policy', value: JSON.stringify(selectedProfile.default_execution_policy ?? {}, null, 2) },
    { label: 'Review policy', value: JSON.stringify(selectedProfile.default_review_policy ?? {}, null, 2) },
    { label: 'Metadata', value: JSON.stringify(selectedProfile.metadata ?? {}, null, 2) },
    { label: 'Expected output schema', value: selectedProfile.expected_output_schema ?? 'Not configured' },
  ] : []

  function templateSummary(value: string | null): string {
    if (!value?.trim()) return 'Not configured'
    return value.length > 180 ? `${value.slice(0, 180)}…` : value
  }
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <Badge variant="secondary">Configuration</Badge>
      <h2 class="text-3xl font-semibold tracking-tight">Agent Profiles</h2>
      <p class="max-w-3xl text-muted-foreground">Inspect execution profiles, client support, prompts, and policy metadata from the External API.</p>
    </div>
    <Button variant="outline" onclick={() => void loadAgentProfiles()}><RefreshCw class="size-4" /> Refresh</Button>
  </div>

  {#if $agentProfilesError}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Unable to load profiles</Alert.Title>
      <Alert.Description>{$agentProfilesError}</Alert.Description>
    </Alert.Root>
  {/if}

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
              onclick={() => selectedProfileId = profile.profile_id}
            >
              <div class="font-medium">{profile.name}</div>
              <div class="truncate text-xs text-muted-foreground">{profile.profile_id} · v{profile.version}</div>
              <div class="mt-2 flex flex-wrap gap-1">
                {#each profile.supported_client_types as client}
                  <Badge variant="secondary">{client}</Badge>
                {/each}
              </div>
            </button>
          {/each}
        </Card.Content>
      </Card.Root>

      {#if selectedProfile}
        <div class="space-y-4">
          <Card.Root>
            <Card.Header>
              <Card.Title>{selectedProfile.name}</Card.Title>
              <Card.Description>{selectedProfile.description ?? 'No description provided.'}</Card.Description>
            </Card.Header>
            <Card.Content>
              <div class="grid gap-3 text-sm md:grid-cols-2 xl:grid-cols-3">
                {#each [
                  ['Profile ID', selectedProfile.profile_id],
                  ['Version', selectedProfile.version],
                  ['Session role', selectedProfile.default_session_role ?? '—'],
                  ['Handle prefix', selectedProfile.handle_prefix ?? '—'],
                  ['State', selectedProfile.active ? 'Active' : 'Archived'],
                  ['Updated', formatDateTime(selectedProfile.updated_at)],
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
            <Card.Header><Card.Title>Client support</Card.Title><Card.Description>Client-specific behavior stays behind adapter/runtime capability boundaries.</Card.Description></Card.Header>
            <Card.Content class="flex flex-wrap gap-2">
              {#each selectedProfile.supported_client_types as client}
                <Badge>{client}</Badge>
              {:else}
                <span class="text-sm text-muted-foreground">No supported client types listed.</span>
              {/each}
            </Card.Content>
          </Card.Root>

          <Card.Root>
            <Card.Header><Card.Title>Prompt templates</Card.Title><Card.Description>Read-only profile detail.</Card.Description></Card.Header>
            <Card.Content class="grid gap-4 xl:grid-cols-2">
              <div class="space-y-2"><div class="text-sm font-medium">System prompt</div><pre class="max-h-72 overflow-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{templateSummary(selectedProfile.system_prompt_template)}</pre></div>
              <div class="space-y-2"><div class="text-sm font-medium">Turn prompt</div><pre class="max-h-72 overflow-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{templateSummary(selectedProfile.turn_prompt_template)}</pre></div>
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
