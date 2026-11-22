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
  import IdentificationCardIcon from 'phosphor-svelte/lib/IdentificationCardIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ProfileForm from './agentProfiles/ProfileForm.svelte'
  import DeleteProfileDialog from './agentProfiles/DeleteProfileDialog.svelte'

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
  let deleting = $state<AgentProfileView | null>(null)
  const selected = $derived(profiles.data?.find((profile) => profile.profile_id === selectedId))
  const editorOpen = $derived(selectedId === '' || !!selected)
  // Svelte Query only notifies about result fields that were read, so read both flags
  // instead of short-circuiting past the client query.
  const loading = $derived([profiles.isPending, clients.isPending].includes(true))
  // Wide screens show the editor beside the list, so start with the first profile open.
  $effect(() => {
    if (selectedId === null && profiles.data?.length && matchMedia('(min-width: 64rem)').matches) selectedId = profiles.data[0].profile_id
  })

  function clientName(profile: AgentProfileView) {
    return clients.data?.find((client) => client.client_type === profile.client_type && client.client_key === profile.client_key)?.name
      ?? `${profile.client_type} / ${profile.client_key}`
  }
</script>

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2 {editorOpen ? 'max-lg:hidden' : ''}">
    <Button disabled={saving || loading || !profiles.data} onclick={() => selectedId = ''}><PlusIcon class="size-4" /> {$t('profiles.new')}</Button>
  </div>

  {#if profiles.isError}
    <Alert.Root variant="destructive"><WarningCircleIcon class="size-4" /><Alert.Description>{profiles.error.message}</Alert.Description></Alert.Root>
  {:else if loading || !profiles.data}
    <div role="status" aria-label={$t('common.loading')} class="grid gap-6 lg:grid-cols-[15rem_minmax(0,1fr)]"><Skeleton class="h-48" /><Skeleton class="h-96" /></div>
  {:else if !profiles.data.length && selectedId !== ''}
    <Empty.Root class="border">
      <Empty.Header>
        <Empty.Media variant="icon"><IdentificationCardIcon /></Empty.Media>
        <Empty.Title>{$t('profiles.emptyTitle')}</Empty.Title>
        <Empty.Description>{$t('profiles.emptyDescription')}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
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
              <span class="block truncate text-xs text-muted-foreground">{profile.model ?? $t('profiles.noLaunch')}</span>
            </span>
          </button>
        {/each}
      </nav>

      {#if editorOpen}
        <div class="min-w-0 space-y-5">
          <Button variant="ghost" size="sm" class="-ml-2 lg:hidden" disabled={saving} onclick={() => selectedId = null}><CaretLeftIcon class="size-4" />{$t('profiles.title')}</Button>
          <div class="min-w-0 space-y-1">
            <h3 class="text-lg font-semibold break-words text-heading">{selected?.name ?? $t('profiles.new')}</h3>
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
            <ProfileForm profile={selected} clients={enabledClients} bind:saving
              oncancel={selected ? undefined : () => selectedId = null}
              onsaved={(saved) => { selectedId = saved.profile_id; toast.success($t('profiles.saved')) }}
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
