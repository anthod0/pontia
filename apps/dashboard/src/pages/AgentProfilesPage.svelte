<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../device/context'
  import type { AgentProfileView } from '../api/types'
  import ClientModelSelect from '../components/chat/ClientModelSelect.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon'
  import { buildAgentProfileInput, createAgentProfileDraft, createAgentProfileDraftFromProfile, type AgentProfileDraftErrors } from './agentProfiles/form'

  const device = useDeviceRuntime()
  const profiles = device.agentProfileQueries.createAgentProfilesQuery()
  const createProfile = device.agentProfileQueries.createAgentProfileMutation()
  const updateProfile = device.agentProfileQueries.updateAgentProfileMutation()
  const deleteProfile = device.agentProfileQueries.deleteAgentProfileMutation()
  const clients = createQuery(
    () => ({ queryKey: ['agent-clients'], queryFn: () => device.api.listAgentClients() }),
    () => device.queryClient,
  )
  const enabledClients = $derived(clients.data?.filter((client) => client.enabled) ?? [])
  let editing = $state(false)
  let formOpen = $state(false)
  let draft = $state(createAgentProfileDraft())
  let errors = $state<AgentProfileDraftErrors>({})
  let error = $state('')
  let modelAvailable = $state(false)
  let deleteOpen = $state(false)
  let deleting = $state<AgentProfileView | null>(null)
  const busy = $derived(createProfile.isPending || updateProfile.isPending || deleteProfile.isPending)
  const selectedClient = $derived(enabledClients.find((client) => client.client_type === draft.client_type && client.client_key === draft.client_key))

  function start(profile?: AgentProfileView) {
    editing = !!profile
    draft = profile ? createAgentProfileDraftFromProfile(profile) : createAgentProfileDraft()
    errors = {}
    error = ''
    modelAvailable = false
    formOpen = true
  }

  function chooseClient(id: string) {
    const client = enabledClients.find((client) => client.id === id)
    draft.client_type = client?.client_type ?? ''
    draft.client_key = client?.client_key ?? ''
    draft.model = ''
    modelAvailable = false
  }

  async function save(event: SubmitEvent) {
    event.preventDefault()
    if (busy) return
    error = ''
    const result = buildAgentProfileInput(draft)
    errors = result.ok ? {} : result.errors
    if (!result.ok) return
    if (draft.client_type && (!selectedClient || !modelAvailable)) {
      errors.model = $t('profiles.clientModelUnavailable')
      return
    }
    try {
      await (editing ? updateProfile : createProfile).mutateAsync(result.input)
      formOpen = false
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    }
  }

  async function remove() {
    if (!deleting || busy) return
    deleteOpen = false
    error = ''
    try {
      await deleteProfile.mutateAsync(deleting.profile_id)
      if (formOpen && editing && draft.profile_id === deleting.profile_id) formOpen = false
      deleting = null
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    }
  }
</script>

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2">
    <Button disabled={busy} onclick={() => start()}><PlusIcon class="size-4" />{$t('profiles.new')}</Button>
    <Button variant="outline" disabled={busy || profiles.isFetching} onclick={() => void profiles.refetch()}><ArrowsClockwiseIcon class="size-4" />{$t('common.refresh')}</Button>
  </div>
  {#if error}<p role="alert" class="text-sm text-destructive">{error}</p>{/if}
  {#if profiles.isError}
    <p role="alert" class="text-sm text-destructive">{profiles.error.message}</p>
  {:else if profiles.isPending}
    <p role="status" class="text-sm text-muted-foreground">{$t('common.loading')}</p>
  {:else}
    <ul aria-label={$t('profiles.title')} class="divide-y rounded-md border">
      {#each profiles.data ?? [] as profile (profile.profile_id)}
        <li class="flex flex-wrap items-center justify-between gap-3 p-4">
          <div class="min-w-0 space-y-1">
            <h3 class="font-medium">{profile.name}</h3>
            <p class="break-all font-mono text-xs text-muted-foreground">{profile.profile_id}</p>
            {#if profile.description}<p class="text-sm text-muted-foreground">{profile.description}</p>{/if}
            {#if profile.client_type}
              <p class="break-all text-xs text-muted-foreground">{clients.data?.find((client) => client.client_type === profile.client_type && client.client_key === profile.client_key)?.name ?? `${profile.client_type} / ${profile.client_key}`} · {profile.model}</p>
            {/if}
          </div>
          <div class="flex gap-2">
            <Button variant="outline" disabled={busy} aria-label={$t('profiles.editNamed', { name: profile.name })} onclick={() => start(profile)}>{$t('common.edit')}</Button>
            <Button variant="ghost" disabled={busy} aria-label={$t('profiles.deleteNamed', { name: profile.name })} onclick={() => { deleting = profile; deleteOpen = true }}>{$t('common.delete')}</Button>
          </div>
        </li>
      {:else}<li class="p-4 text-sm text-muted-foreground">{$t('profiles.empty')}</li>{/each}
    </ul>
  {/if}

  {#if formOpen}
    <form class="grid max-w-2xl gap-4" onsubmit={save} novalidate>
      <h3 class="font-semibold">{$t(editing ? 'profiles.edit' : 'profiles.new')}</h3>
      <div class="grid gap-2">
        <Label for="profile-id">{$t('profiles.id')}</Label>
        <Input id="profile-id" bind:value={draft.profile_id} readonly={editing} disabled={busy} aria-invalid={!!errors.profile_id} />
        {#if errors.profile_id}<p role="alert" class="text-sm text-destructive">{errors.profile_id}</p>{/if}
      </div>
      <div class="grid gap-2">
        <Label for="profile-name">{$t('profiles.name')}</Label>
        <Input id="profile-name" bind:value={draft.name} disabled={busy} aria-invalid={!!errors.name} />
        {#if errors.name}<p role="alert" class="text-sm text-destructive">{errors.name}</p>{/if}
      </div>
      <div class="grid gap-2">
        <Label for="profile-description">{$t('profiles.description')}</Label>
        <Textarea id="profile-description" bind:value={draft.description} rows={2} disabled={busy} />
      </div>
      <div class="grid gap-2">
        <Label for="profile-client">{$t('clients.agentClient')}</Label>
        <Select.Root type="single" value={selectedClient?.id ?? ''} onValueChange={chooseClient} disabled={busy}>
          <Select.Trigger id="profile-client" class="w-full">{selectedClient?.name ?? (draft.client_type ? `${draft.client_type} / ${draft.client_key}` : $t('profiles.noClient'))}</Select.Trigger>
          <Select.Content>
            <Select.Item value="" label={$t('profiles.noClient')}>{$t('profiles.noClient')}</Select.Item>
            {#each enabledClients as client (client.id)}<Select.Item value={client.id} label={client.name}>{client.name}</Select.Item>{/each}
          </Select.Content>
        </Select.Root>
        {#if draft.client_type}<Button type="button" variant="ghost" class="justify-self-start" disabled={busy} onclick={() => chooseClient('')}>{$t('profiles.clearClient')}</Button>{/if}
        {#if clients.isError}
          <p role="alert" class="text-sm text-destructive">{clients.error.message}</p>
          <Button type="button" variant="outline" onclick={() => void clients.refetch()}>{$t('common.retry')}</Button>
        {/if}
      </div>
      {#if selectedClient}
        <div class="grid gap-2">
          <Label for="profile-model">{$t('model.title')}</Label>
          {#key selectedClient.id}<ClientModelSelect id="profile-model" client={selectedClient} bind:value={draft.model} bind:available={modelAvailable} disabled={busy} />{/key}
        </div>
      {:else if draft.client_type}<p role="status" class="text-sm text-muted-foreground">{$t('profiles.clientModelUnavailable')}</p>{/if}
      {#if errors.model}<p role="alert" class="text-sm text-destructive">{errors.model}</p>{/if}
      <div class="grid gap-2">
        <Label for="profile-prompt">{$t('profiles.systemPrompt')}</Label>
        <Textarea id="profile-prompt" bind:value={draft.system_prompt} rows={10} disabled={busy} aria-invalid={!!errors.system_prompt} />
        {#if errors.system_prompt}<p role="alert" class="text-sm text-destructive">{errors.system_prompt}</p>{/if}
      </div>
      <div class="flex justify-end gap-2">
        <Button type="button" variant="outline" disabled={busy} onclick={() => formOpen = false}>{$t('common.cancel')}</Button>
        <Button type="submit" disabled={busy}>{$t('common.save')}</Button>
      </div>
    </form>
  {/if}
</section>

<AlertDialog.Root bind:open={deleteOpen}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{$t('profiles.deleteConfirm')}</AlertDialog.Title>
      <AlertDialog.Description>{$t('profiles.deleteDescription', { name: deleting?.name ?? '' })}</AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel>
      <AlertDialog.Action onclick={remove}>{$t('common.delete')}</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
