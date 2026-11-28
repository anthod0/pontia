<script lang="ts">
  import { t } from '$lib/i18n'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ProfileForm from '../../../pages/agentProfiles/ProfileForm.svelte'
  import DeleteProfileDialog from '../../../pages/agentProfiles/DeleteProfileDialog.svelte'
  import TemplateCards from '../../../pages/agentProfiles/TemplateCards.svelte'
  import { createAgentProfiles } from '../../../features/settings/agentProfiles.svelte'
  const settings = createAgentProfiles()
  const isSplitView = () => matchMedia('(min-width: 64rem)').matches
  $effect(() => {
    if (settings.selectedId === null && settings.profiles.data?.length && isSplitView())
      settings.selectedId = settings.profiles.data[0].profile_id
  })
</script>

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2 {settings.editorOpen ? 'max-lg:hidden' : ''}">
    <Button disabled={settings.creationDisabled} onclick={() => settings.openNew()}
      ><PlusIcon data-icon="inline-start" />{$t('profiles.new')}</Button
    >
  </div>

  {#if settings.profiles.isError}
    <Alert.Root variant="destructive"
      ><WarningCircleIcon class="size-4" /><Alert.Description>{settings.profiles.error.message}</Alert.Description
      ></Alert.Root
    >
  {:else if settings.loading || !settings.profiles.data}
    <div role="status" aria-label={$t('common.loading')} class="grid gap-6 lg:grid-cols-[15rem_minmax(0,1fr)]">
      <Skeleton class="h-48" /><Skeleton class="h-96" />
    </div>
  {:else if !settings.profiles.data.length && settings.selectedId !== ''}
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
      <TemplateCards clients={settings.clients.data ?? []} onchoose={settings.openNew} class="lg:grid-cols-3" />
    </div>
  {:else}
    <div class="grid grid-cols-[minmax(0,1fr)] items-start gap-6 lg:grid-cols-[15rem_minmax(0,1fr)]">
      <nav aria-label={$t('profiles.title')} class="space-y-1 {settings.editorOpen ? 'max-lg:hidden' : ''}">
        {#each settings.profiles.data as profile (profile.profile_id)}
          <button
            type="button"
            class="flex w-full items-start gap-2.5 rounded-item px-3 py-2 text-left text-sm transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-60 {settings.selectedId ===
            profile.profile_id
              ? 'bg-muted'
              : ''}"
            aria-current={settings.selectedId === profile.profile_id ? 'true' : undefined}
            disabled={settings.saving}
            onclick={() => (settings.selectedId = profile.profile_id)}
          >
            <ClientIcon
              clientType={profile.client_type ?? undefined}
              clientKey={profile.client_key}
              class="mt-0.5 size-4 shrink-0 text-foreground"
            />
            <span class="min-w-0 flex-1">
              <span
                class="block truncate {settings.selectedId === profile.profile_id
                  ? 'font-medium text-heading'
                  : ''}">{profile.name}</span
              >
              <span class="flex gap-1 text-xs text-muted-foreground">
                <span class="truncate">{profile.model ?? $t('profiles.noLaunch')}</span>
                {#if profile.use_yolo}<span class="shrink-0">· {$t('profiles.yoloMode')}</span>{/if}
              </span>
            </span>
          </button>
        {/each}
      </nav>

      {#if settings.editorOpen}
        <div class="min-w-0 space-y-5">
          <Button
            variant="ghost"
            size="sm"
            class="-ml-2 lg:hidden"
            disabled={settings.saving}
            onclick={() => (settings.selectedId = null)}
            ><CaretLeftIcon data-icon="inline-start" />{$t('profiles.title')}</Button
          >
          <div class="min-w-0 space-y-1">
            <h3 class="text-lg font-semibold break-words text-heading">
              {settings.selected?.name ?? $t('profiles.new')}
            </h3>
            {#if settings.selected?.client_type}
              <p class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                <ClientIcon
                  clientType={settings.selected.client_type}
                  clientKey={settings.selected.client_key}
                  class="size-3.5 shrink-0 text-foreground"
                />
                <span class="truncate"
                  >{settings.clientName(settings.selected)} ·
                  <span class="font-mono">{settings.selected.model}</span></span
                >
              </p>
            {/if}
          </div>
          {#if settings.clients.isError}
            <Alert.Root variant="destructive">
              <WarningCircleIcon class="size-4" />
              <Alert.Description>{settings.clients.error.message}</Alert.Description>
              <Button
                variant="outline"
                size="sm"
                class="mt-2 justify-self-start"
                onclick={() => void settings.clients.refetch()}>{$t('common.retry')}</Button
              >
            </Alert.Root>
          {/if}
          {#key settings.selectedId}
            <ProfileForm
              profile={settings.selected}
              startDraft={settings.startDraft}
              clients={settings.enabledClients}
              allClients={settings.clients.data ?? []}
              template={settings.selected ? undefined : settings.startTemplate}
              bind:saving={settings.saving}
              oncancel={settings.selected ? undefined : () => (settings.selectedId = null)}
              onsaved={(profile) => settings.profileSaved(isSplitView() ? profile.profile_id : null)}
              ondelete={(profile) => (settings.deleting = profile)}
            />
          {/key}
        </div>
      {:else}
        <p class="py-2 text-sm text-muted-foreground max-lg:hidden">{$t('profiles.select')}</p>
      {/if}
    </div>
  {/if}
</section>

<DeleteProfileDialog
  bind:profile={settings.deleting}
  ondeleted={(profile) => {
    if (settings.selectedId === profile.profile_id) settings.selectedId = null
  }}
/>
