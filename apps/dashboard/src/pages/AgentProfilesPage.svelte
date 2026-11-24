<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import { useDeviceRuntime } from '../device/context'
  import type { AgentProfileView } from '../api/types'
  import ClientIcon from '../components/chat/ClientIcon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { useTopBarAction } from '../components/layout/topBarAction'
  import { useSettingsDetail } from '../components/settings/settingsDetail'
  import ProfileForm from './agentProfiles/ProfileForm.svelte'
  import DeleteProfileDialog from './agentProfiles/DeleteProfileDialog.svelte'
  import TemplateCards from './agentProfiles/TemplateCards.svelte'
  import type { ProfileTemplate } from './agentProfiles/templates'

  const device = useDeviceRuntime()
  const profiles = device.agentProfileQueries.createAgentProfilesQuery()
  const clients = createQuery(
    () => ({ queryKey: ['agent-clients'], queryFn: () => device.api.listAgentClients() }),
    () => device.queryClient,
  )
  const enabledClients = $derived(clients.data?.filter((client) => client.enabled) ?? [])
  // `null` selects nothing; `''` opens the new-profile editor.
  let selectedId = $state<string | null>(null)
  let saving = $state(false)
  // A template chosen from the empty state, applied when the new-profile editor opens.
  let startTemplate = $state<ProfileTemplate>()
  let deleting = $state<AgentProfileView | null>(null)
  const selected = $derived(profiles.data?.find((profile) => profile.profile_id === selectedId))
  const editorOpen = $derived(selectedId === '' || !!selected)
  // Svelte Query only notifies about result fields that were read, so read both flags
  // instead of short-circuiting past the client query.
  const loading = $derived([profiles.isPending, clients.isPending].includes(true))
  const isSplitView = () => matchMedia('(min-width: 64rem)').matches
  // Wide screens show the editor beside the list, so start with the first profile open.
  $effect(() => {
    if (selectedId === null && profiles.data?.length && isSplitView()) selectedId = profiles.data[0].profile_id
  })

  // Phones put the page action and the open profile in the top bar.
  const topBarAction = useTopBarAction()
  const settingsDetail = useSettingsDetail()
  const creationDisabled = $derived(saving || loading || !profiles.data)
  $effect(() => {
    topBarAction?.set(editorOpen ? null : newProfileAction)
    settingsDetail?.set(editorOpen ? { title: selected?.name ?? $t('profiles.new'), back: () => selectedId = null, disabled: saving } : null)
  })
  $effect(() => () => {
    topBarAction?.set(null)
    settingsDetail?.set(null)
  })

  function openNew(template?: ProfileTemplate) {
    startTemplate = template
    selectedId = ''
  }

  function profileSaved(profile: AgentProfileView) {
    selectedId = isSplitView() ? profile.profile_id : null
    toast.success($t('profiles.saved'))
  }

  function clientName(profile: AgentProfileView) {
    return clients.data?.find((client) => client.client_type === profile.client_type && client.client_key === profile.client_key)?.name
      ?? `${profile.client_type} / ${profile.client_key}`
  }
</script>

{#snippet newProfileAction()}
  <Button class="mr-3 h-10 shrink-0 px-3 max-md:mt-2 md:hidden" disabled={creationDisabled} onclick={() => openNew()}><PlusIcon data-icon="inline-start" />{$t('profiles.newShort')}</Button>
{/snippet}

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2 max-md:hidden {editorOpen ? 'max-lg:hidden' : ''}">
    <Button disabled={creationDisabled} onclick={() => openNew()}><PlusIcon data-icon="inline-start" />{$t('profiles.new')}</Button>
  </div>

  {#if profiles.isError}
    <Alert.Root variant="destructive"><WarningCircleIcon class="size-4" /><Alert.Description>{profiles.error.message}</Alert.Description></Alert.Root>
  {:else if loading || !profiles.data}
    <div role="status" aria-label={$t('common.loading')} class="grid gap-6 lg:grid-cols-[15rem_minmax(0,1fr)]"><Skeleton class="h-48" /><Skeleton class="h-96" /></div>
  {:else if !profiles.data.length && selectedId !== ''}
    <Empty.Root class="border">
      <Empty.Header>
        <Empty.Media variant="icon"><RobotIcon /></Empty.Media>
        <Empty.Title>{$t('profiles.emptyTitle')}</Empty.Title>
        <Empty.Description>{$t('profiles.emptyDescription')}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
    <div class="grid gap-3">
      <div class="grid gap-1">
        <p class="text-sm font-medium text-heading">{$t('profiles.templatesTitle')}</p>
        <p class="text-xs text-muted-foreground">{$t('profiles.templatesEmptyHint')}</p>
      </div>
      <TemplateCards clients={clients.data ?? []} onchoose={openNew} class="lg:grid-cols-3" />
    </div>
  {:else}
    <div class="grid grid-cols-[minmax(0,1fr)] items-start gap-6 lg:grid-cols-[15rem_minmax(0,1fr)]">
      <nav aria-label={$t('profiles.title')} class="space-y-1 {editorOpen ? 'max-lg:hidden' : ''}">
        {#each profiles.data as profile (profile.profile_id)}
          <button type="button"
            class="flex w-full items-start gap-2.5 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-60 {selectedId === profile.profile_id ? 'bg-muted' : ''}"
            aria-current={selectedId === profile.profile_id ? 'true' : undefined} disabled={saving} onclick={() => selectedId = profile.profile_id}>
            <ClientIcon clientType={profile.client_type ?? undefined} acpAgentId={profile.client_key} class="mt-0.5 size-4 shrink-0 text-foreground" />
            <span class="min-w-0 flex-1">
              <span class="block truncate {selectedId === profile.profile_id ? 'font-medium text-heading' : ''}">{profile.name}</span>
              <span class="flex gap-1 text-xs text-muted-foreground">
                <span class="truncate">{profile.model ?? $t('profiles.noLaunch')}</span>
                {#if profile.use_yolo}<span class="shrink-0">· {$t('profiles.yoloMode')}</span>{/if}
              </span>
            </span>
          </button>
        {/each}
      </nav>

      {#if editorOpen}
        <div class="min-w-0 space-y-5">
          <Button variant="ghost" size="sm" class="-ml-2 max-md:hidden lg:hidden" disabled={saving} onclick={() => selectedId = null}><CaretLeftIcon data-icon="inline-start" />{$t('profiles.title')}</Button>
          <div class="min-w-0 space-y-1">
            <h3 class="text-lg max-md:sr-only font-semibold break-words text-heading">{selected?.name ?? $t('profiles.new')}</h3>
            {#if selected?.client_type}
              <p class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                <ClientIcon clientType={selected.client_type} acpAgentId={selected.client_key} class="size-3.5 shrink-0 text-foreground" />
                <span class="truncate">{clientName(selected)} · <span class="font-mono">{selected.model}</span></span>
              </p>
            {/if}
          </div>
          {#if clients.isError}
            <Alert.Root variant="destructive">
              <WarningCircleIcon class="size-4" />
              <Alert.Description>{clients.error.message}</Alert.Description>
              <Button variant="outline" size="sm" class="mt-2 justify-self-start" onclick={() => void clients.refetch()}>{$t('common.retry')}</Button>
            </Alert.Root>
          {/if}
          {#key selectedId}
            <ProfileForm profile={selected} clients={enabledClients} allClients={clients.data ?? []} template={selected ? undefined : startTemplate} bind:saving
              oncancel={selected ? undefined : () => selectedId = null}
              onsaved={profileSaved}
              ondelete={(profile) => deleting = profile} />
          {/key}
        </div>
      {:else}
        <p class="py-2 text-sm text-muted-foreground max-lg:hidden">{$t('profiles.select')}</p>
      {/if}
    </div>
  {/if}
</section>

<DeleteProfileDialog bind:profile={deleting} ondeleted={(profile) => { if (selectedId === profile.profile_id) selectedId = null }} />
