<script lang="ts">
  import { untrack } from 'svelte'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentClientView, AgentProfileView } from '../../api/types'
  import ClientIcon from '../../components/chat/ClientIcon.svelte'
  import ClientModelSelect from '../../components/chat/ClientModelSelect.svelte'
  import ReasoningEffortSelect from '../../components/chat/ReasoningEffortSelect.svelte'
  import ModelSelectPlaceholder from '../../components/chat/ModelSelectPlaceholder.svelte'
  import { copyText } from '../../lib/copyText'
  import { dashboardPath } from '$lib/navigation'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import { buildAgentProfileInput, createAgentProfileDraft, createAgentProfileDraftFromProfile, profileIdFromName, type AgentProfileDraftErrors } from './form'

  let { profile, clients, oncancel, onsaved, ondelete, saving = $bindable(false) }: {
    profile?: AgentProfileView
    clients: AgentClientView[]
    oncancel?: () => void
    onsaved: (profile: AgentProfileView) => void
    ondelete?: (profile: AgentProfileView) => void
    saving?: boolean
  } = $props()
  const id = $props.id()
  const device = useDeviceRuntime()
  const createProfile = device.agentProfileQueries.createAgentProfileMutation()
  const updateProfile = device.agentProfileQueries.updateAgentProfileMutation()

  // Callers re-key the form per profile, so it seeds its draft once.
  const initial = untrack(() => profile)
  let draft = $state(initial ? createAgentProfileDraftFromProfile(initial) : createAgentProfileDraft())
  let idEdited = $state(false)
  let copied = $state(false)
  let errors = $state<AgentProfileDraftErrors>({})
  let error = $state('')
  let effortUnavailable = $state(false)
  let modelAvailable = $state(false)
  const selectedClient = $derived(clients.find((client) => client.client_type === draft.client_type && client.client_key === draft.client_key))

  $effect(() => {
    if (!initial && !draft.client_type && clients[0]) untrack(() => chooseClient(clients[0].id))
  })

  function chooseClient(clientId: string) {
    const client = clients.find((client) => client.id === clientId)
    if (!client) return
    draft.client_type = client.client_type
    draft.client_key = client.client_key
    draft.model = ''
    draft.reasoning_effort = ''
    effortUnavailable = false
    modelAvailable = false
  }

  async function copyId() {
    if (!profile) return
    copied = await copyText(profile.profile_id)
    if (copied) setTimeout(() => copied = false, 2000)
  }

  async function save(event: SubmitEvent) {
    event.preventDefault()
    if (saving || effortUnavailable) return
    error = ''
    const result = buildAgentProfileInput(draft, { creating: !profile })
    errors = result.ok ? {} : result.errors
    if (!result.ok) return
    if (!selectedClient || !modelAvailable) {
      // A saved client that is no longer offered already shows the same message as a status.
      if (selectedClient) errors.model = $t('profiles.clientModelUnavailable')
      return
    }
    saving = true
    try {
      onsaved(await (profile ? updateProfile : createProfile).mutateAsync(result.input))
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      saving = false
    }
  }
</script>

<form class="grid gap-5" onsubmit={save} novalidate>
  <div class="grid gap-4 sm:grid-cols-2">
    <div class="grid content-start gap-2">
      <Label for={`${id}-name`}>{$t('profiles.name')}</Label>
      <Input id={`${id}-name`} bind:value={draft.name} disabled={saving} aria-invalid={!!errors.name}
        oninput={() => { if (!profile && !idEdited) draft.profile_id = profileIdFromName(draft.name) }} />
      {#if errors.name}<p role="alert" class="text-sm text-destructive">{errors.name}</p>{/if}
    </div>
    <div class="grid content-start gap-2">
      {#if profile}
        <Label for={`${id}-id`}>{$t('profiles.id')}</Label>
        <div class="flex min-w-0 items-center gap-1">
          <Input id={`${id}-id`} value={profile.profile_id} readonly class="border-transparent bg-transparent px-0 font-mono text-muted-foreground shadow-none dark:bg-transparent" title={profile.profile_id} />
          <Button type="button" variant="ghost" size="icon-sm" class="shrink-0" aria-label={$t(copied ? 'common.copied' : 'common.copyNamed', { value: profile.profile_id })} onclick={copyId}>
            {#if copied}<CheckIcon class="size-4" />{:else}<CopyIcon class="size-4" />{/if}
          </Button>
        </div>
      {:else}
        <Label for={`${id}-id`}>{$t('profiles.id')}</Label>
        <Input id={`${id}-id`} bind:value={draft.profile_id} class="font-mono" autocapitalize="off" spellcheck={false} disabled={saving} aria-invalid={!!errors.profile_id} oninput={() => idEdited = draft.profile_id !== ''} />
        {#if errors.profile_id}<p role="alert" class="text-sm text-destructive">{errors.profile_id}</p>{/if}
      {/if}
    </div>
  </div>

  <div class="grid gap-2">
    <Label for={`${id}-description`}>{$t('profiles.description')}</Label>
    <Textarea id={`${id}-description`} bind:value={draft.description} rows={2} disabled={saving} />
    <p class="text-xs text-muted-foreground">{$t('profiles.descriptionHint')}</p>
  </div>

  <div class="grid gap-4 sm:grid-cols-2">
    <div class="grid content-start gap-2">
      <Label for={`${id}-client`}>{$t('clients.agentClient')}</Label>
      <Select.Root type="single" value={selectedClient?.id ?? ''} onValueChange={chooseClient} disabled={saving}>
        <Select.Trigger id={`${id}-client`} class="w-full">
          <span class="flex min-w-0 items-center gap-2">
            {#if selectedClient}<ClientIcon clientType={selectedClient.client_type} acpAgentId={selectedClient.client_key} class="size-4 text-foreground" />{/if}
            <span class="truncate">{selectedClient?.name ?? (draft.client_type ? `${draft.client_type} / ${draft.client_key}` : $t('clients.choose'))}</span>
          </span>
        </Select.Trigger>
        <Select.Content>
          {#each clients as client (client.id)}
            <Select.Item value={client.id} label={client.name}><ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-4 text-foreground" />{client.name}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
      {#if !clients.length}<p role="status" class="text-sm text-muted-foreground">{$t('profiles.noClients')} <a class="underline underline-offset-4" href={dashboardPath('/settings/clients')}>{$t('settings.agentClients')}</a></p>{/if}
    </div>
    <div class="grid content-start gap-2">
      <Label for={`${id}-model`}>{$t('model.title')}</Label>
      {#if selectedClient}
        {#key selectedClient.id}
          <ClientModelSelect id={`${id}-model`} client={selectedClient} bind:value={draft.model} bind:available={modelAvailable} disabled={saving}>
            {#snippet children(selectedModel)}
              {#if selectedModel}
                <ReasoningEffortSelect class="w-full" options={selectedModel.reasoning_efforts ?? []} defaultEffort={selectedModel.default_reasoning_effort}
                  savedValue={profile?.model === draft.model ? profile.reasoning_effort : null} bind:value={draft.reasoning_effort} bind:unavailable={effortUnavailable} disabled={saving} />
              {/if}
            {/snippet}
          </ClientModelSelect>
        {/key}
      {:else}
        <ModelSelectPlaceholder id={`${id}-model`} label={draft.model || $t('model.choose')} />
      {/if}
    </div>
  </div>
  {#if draft.client_type && !selectedClient}<p role="status" class="-mt-2 text-sm text-muted-foreground">{$t('profiles.clientModelUnavailable')}</p>{/if}
  {#if errors.model}<p role="alert" class="-mt-2 text-sm text-destructive">{errors.model}</p>{/if}

  <div class="flex items-center gap-2">
    <Switch id={`${id}-yolo`} bind:checked={draft.use_yolo} disabled={saving} />
    <Label for={`${id}-yolo`}>{$t('profiles.useYolo')}</Label>
  </div>

  <div class="grid gap-2">
    <Label for={`${id}-prompt`}>{$t('profiles.systemPrompt')}</Label>
    <Textarea id={`${id}-prompt`} bind:value={draft.system_prompt} rows={8} spellcheck={false} class="min-h-40 font-mono text-xs leading-5" disabled={saving} aria-invalid={!!errors.system_prompt} />
    {#if errors.system_prompt}<p role="alert" class="text-sm text-destructive">{errors.system_prompt}</p>
    {:else}<p class="text-xs text-muted-foreground">{$t('profiles.promptHint')}</p>{/if}
  </div>

  {#if error}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
  <div class="flex flex-wrap justify-end gap-2">
    {#if profile && ondelete}
      <Button type="button" variant="ghost" class="text-destructive hover:text-destructive" disabled={saving} aria-label={$t('profiles.deleteNamed', { name: profile.name })} onclick={() => profile && ondelete?.(profile)}><TrashIcon data-icon="inline-start" />{$t('common.delete')}</Button>
    {/if}
    {#if oncancel}<Button type="button" variant="outline" disabled={saving} onclick={oncancel}>{$t('common.cancel')}</Button>{/if}
    <Button type="submit" disabled={saving || effortUnavailable || (!profile && !clients.length)}>{$t(profile ? 'common.save' : 'profiles.create')}</Button>
  </div>
</form>
