<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import { untrack } from 'svelte'
  import { t, type MessageKey } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentClientView, AgentProfileView } from '../../api/types'
  import ClientIcon from '../../components/chat/ClientIcon.svelte'
  import ModelEffortPicker from '../../components/chat/ModelEffortPicker.svelte'
  import ModelSelectPlaceholder from '../../components/chat/ModelSelectPlaceholder.svelte'
  import { copyText } from '../../lib/copyText'
  import { dashboardPath } from '$lib/navigation'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import { Separator } from '$lib/components/ui/separator/index.js'
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon'
  import SwapIcon from 'phosphor-svelte/lib/SwapIcon'
  import { modelCatalogQuery } from '../../queries/modelCatalog'
  import TemplateCards from './TemplateCards.svelte'
  import { applyProfileTemplate, draftChanged, templateClient, templateClientName, templateClientStatus, templateIcon, templateModel, type ProfileTemplate } from './templates'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import { buildAgentProfileInput, createAgentProfileDraft, createAgentProfileDraftFromProfile, profileIdFromName, type AgentProfileDraftErrors } from './form'

  let { profile, clients, allClients = clients, template, oncancel, onsaved, ondelete, saving = $bindable(false) }: {
    profile?: AgentProfileView
    /** Enabled clients the profile can launch with. */
    clients: AgentClientView[]
    /** Every known client, used to tell a disabled template client from a missing one. */
    allClients?: AgentClientView[]
    /** A template to apply as soon as a new-profile form opens. */
    template?: ProfileTemplate
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

  // New profiles can start from a template. The baseline holds the values the form was last reset
  // to or filled with, so replacing a changed draft asks first.
  let baseline = $state(createAgentProfileDraft())
  // The client and model fill in on their own after a reset or a template; until the model settles
  // they are copied into the baseline instead of counting as the user's changes.
  let settling = $state(true)
  let applied = $state<ProfileTemplate | null>(null)
  let pickerOpen = $state(true)
  let replacing = $state<ProfileTemplate | null>(null)
  let applying = 0
  let fill = $state<{ kind: 'missing' | 'disabled' | 'model'; template: ProfileTemplate } | null>(null)
  // The note describes the client and model the template left; it goes away once the user changes them.
  const fillNote = $derived(fill && (['client_type', 'client_key', 'model'] as const).every((field) => draft[field] === baseline[field]) ? fill : null)

  $effect(() => {
    if (!initial && !draft.client_type && clients[0]) untrack(() => chooseClient(clients[0].id))
  })
  $effect(() => {
    if (initial || !settling) return
    baseline.client_type = draft.client_type
    baseline.client_key = draft.client_key
    baseline.model = draft.model
    baseline.reasoning_effort = draft.reasoning_effort
    if (modelAvailable) settling = false
  })
  $effect(() => {
    if (!initial && template) untrack(() => void applyTemplate(template))
  })

  function chooseTemplate(next: ProfileTemplate) {
    if (saving) return
    const changed = draftChanged(draft, baseline)
    if (next.id === applied?.id && !changed) pickerOpen = false
    else if (changed) replacing = next
    else void applyTemplate(next)
  }

  async function applyTemplate(next: ProfileTemplate) {
    replacing = null
    const attempt = ++applying
    const client = templateClient(next, clients)
    let resolved = { model: next.model, reasoning_effort: next.reasoning_effort ?? '', fallback: false }
    if (client) {
      try {
        resolved = templateModel(next, await device.queryClient.ensureQueryData(modelCatalogQuery(device, client)))
      } catch {
        // The model picker reports catalog errors itself.
      }
    }
    if (attempt !== applying) return
    const filled = applyProfileTemplate(draft, next, clients)
    if (client) Object.assign(filled, { model: resolved.model, reasoning_effort: resolved.reasoning_effort })
    if (filled.client_type !== draft.client_type || filled.client_key !== draft.client_key) {
      effortUnavailable = false
      modelAvailable = false
    }
    draft = filled
    baseline = { ...filled }
    settling = true
    idEdited = true
    errors = {}
    applied = next
    pickerOpen = false
    const status = templateClientStatus(next, allClients)
    const kind = status === 'available' ? (resolved.fallback ? 'model' : null) : status
    fill = kind ? { kind, template: next } : null
  }

  /** Splits a translated message around its placeholders so they can be rendered as markup. */
  function messageParts(key: MessageKey) {
    const marked = $t(key, { client: '{client}', model: '{model}', link: '{link}' })
    return marked.split(/(\{client\}|\{model\}|\{link\})/).filter(Boolean)
  }

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

{#if !initial}
  {#if applied && !pickerOpen}
    {@const AppliedIcon = templateIcon(applied)}
    <div class="flex min-w-0 items-center gap-2.5 rounded-lg border bg-muted/40 px-3 py-2">
      <span class="flex size-7 shrink-0 items-center justify-center rounded-md bg-background text-foreground"><AppliedIcon class="size-4" /></span>
      <p class="min-w-0 flex-1 text-sm">
        <span class="text-muted-foreground">{$t('profiles.templateApplied')}</span>
        <span class="font-medium text-heading">{applied.text.en.name}</span><span class="text-muted-foreground max-sm:hidden">{$t('profiles.templateAppliedHint')}</span>
      </p>
      <Button type="button" variant="ghost" size="sm" class="shrink-0" disabled={saving} onclick={() => pickerOpen = true}><SwapIcon data-icon="inline-start" />{$t('profiles.changeTemplate')}</Button>
    </div>
  {:else}
    <div class="grid gap-3">
      <div class="flex items-end justify-between gap-2">
        <div class="grid gap-1">
          <p class="text-sm font-medium text-heading">{$t('profiles.templatesTitle')}</p>
          <p class="text-xs text-muted-foreground">{$t('profiles.templatesHint')}</p>
        </div>
        {#if applied}<Button type="button" variant="ghost" size="sm" class="shrink-0" onclick={() => pickerOpen = false}>{$t('profiles.hideTemplates')}</Button>{/if}
      </div>
      <TemplateCards clients={allClients} selected={applied?.id} onchoose={chooseTemplate} class="xl:grid-cols-3" />
    </div>
  {/if}
  <Separator />
{/if}

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
      <ResponsiveSelect value={selectedClient?.id ?? ''} onValueChange={(clientId) => { settling = false; chooseClient(clientId) }} disabled={saving}
        label={$t('clients.agentClient')} triggerClass="w-full" id={`${id}-client`}
        options={clients} keyOf={client => client.id} labelOf={client => client.name}>
        {#snippet trigger()}
          <span class="flex min-w-0 items-center gap-2">
            {#if selectedClient}<ClientIcon clientType={selectedClient.client_type} acpAgentId={selectedClient.client_key} class="size-4 text-foreground" />{/if}
            <span class="truncate">{selectedClient?.name ?? (draft.client_type ? `${draft.client_type} / ${draft.client_key}` : $t('clients.choose'))}</span>
          </span>
        {/snippet}
        {#snippet option(client)}<ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-4 text-foreground" />{client.name}{/snippet}
      </ResponsiveSelect>
      {#if !clients.length}<p role="status" class="text-sm text-muted-foreground">{$t('profiles.noClients')} <a class="underline underline-offset-4" href={dashboardPath('/settings/clients')}>{$t('settings.agentClients')}</a></p>{/if}
    </div>
    <div class="grid content-start gap-2">
      <Label for={`${id}-model`}>{$t('model.title')}</Label>
      {#if selectedClient}
        {#key selectedClient.id}
          <ModelEffortPicker id={`${id}-model`} client={selectedClient} bind:model={draft.model} bind:effort={draft.reasoning_effort}
            savedEffort={profile?.model === draft.model ? profile.reasoning_effort : null} bind:available={modelAvailable} bind:effortUnavailable disabled={saving} />
        {/key}
      {:else}
        <ModelSelectPlaceholder id={`${id}-model`} label={draft.model || $t('model.choose')} />
      {/if}
    </div>
  </div>
  {#if draft.client_type && !selectedClient}<p role="status" class="-mt-2 text-sm text-muted-foreground">{$t('profiles.clientModelUnavailable')}</p>{/if}
  {#if fillNote}
    {@const clientName = templateClientName(fillNote.template, allClients)}
    <p role="status" class="-mt-2 flex items-start gap-1.5 text-sm text-muted-foreground">
      <InfoIcon class="mt-0.5 size-4 shrink-0" />
      <span>
        {#each messageParts(fillNote.kind === 'model' ? 'profiles.templateModelMissing' : fillNote.kind === 'missing' ? 'profiles.templateClientMissing' : 'profiles.templateClientDisabled') as part, index (index)}
          {#if part === '{client}'}<span class={fillNote.kind === 'model' ? undefined : 'font-medium text-foreground'}>{clientName}</span>
          {:else if part === '{model}'}<span class="font-mono text-xs">{fillNote.template.model}</span>
          {:else if part === '{link}'}<a class="underline underline-offset-4" href={dashboardPath('/settings/clients')}>{$t('settings.agentClients')}</a>
          {:else}{part}{/if}
        {/each}
      </span>
    </p>
  {/if}
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
  <div class="profile-actions flex flex-wrap justify-end gap-2">
    {#if profile && ondelete}
      <Button type="button" variant="ghost" class="text-destructive hover:text-destructive" disabled={saving} aria-label={$t('profiles.deleteNamed', { name: profile.name })} onclick={() => profile && ondelete?.(profile)}><TrashIcon data-icon="inline-start" />{$t('common.delete')}</Button>
    {/if}
    {#if oncancel}<Button type="button" variant="outline" disabled={saving} onclick={oncancel}>{$t('common.cancel')}</Button>{/if}
    <Button type="submit" disabled={saving || effortUnavailable || (!profile && !clients.length)}>{$t(profile ? 'common.save' : 'profiles.create')}</Button>
  </div>
</form>

<AlertDialog.Root open={!!replacing} onOpenChange={(open) => { if (!open) replacing = null }}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{$t('profiles.replaceTemplateTitle')}</AlertDialog.Title>
      <AlertDialog.Description>{$t('profiles.replaceTemplateDescription', { name: replacing?.text.en.name ?? '' })}</AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel>
      <AlertDialog.Action onclick={() => replacing && void applyTemplate(replacing)}>{$t('profiles.replace')}</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

<style>
  :global(.mobile-screen) form { padding-bottom: calc(4rem + env(safe-area-inset-bottom)) }
  :global(.mobile-screen) .profile-actions {
    position: absolute;
    inset: auto 0 0;
    z-index: 1;
    padding: .75rem 1rem max(.75rem, env(safe-area-inset-bottom));
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
</style>
