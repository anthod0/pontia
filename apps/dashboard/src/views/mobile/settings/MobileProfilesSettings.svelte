<script lang="ts">
  import { useMobileHeader } from '../../../shell/mobile/header'
  import { t } from '$lib/i18n'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ProfileForm from '../../../pages/agentProfiles/ProfileForm.svelte'
  import DeleteProfileDialog from '../../../pages/agentProfiles/DeleteProfileDialog.svelte'
  import TemplateCards from '../../../pages/agentProfiles/TemplateCards.svelte'
  import type { ProfileTemplate } from '../../../pages/agentProfiles/templates'
  import { createAgentProfiles } from '../../../features/settings/agentProfiles.svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  const settings = createAgentProfiles(openEditor)
  const stack = useMobileStack()!
  const { action } = useMobileHeader()
  let editorId: string
  function openEditor(title: string) {
    editorId = stack.open({ title, content: editor, dismiss: () => { settings.selectedId = null } })
  }
  function newProfile(template?: ProfileTemplate) {
    settings.openNew(template)
    openEditor($t('profiles.new'))
  }
  $effect(() => {
    action.set(add)
    return () => action.set(null)
  })
</script>

{#if settings.profiles.isError}
  <Alert.Root variant="destructive"
    ><WarningCircleIcon class="size-4" /><Alert.Description>{settings.profiles.error.message}</Alert.Description
    ></Alert.Root
  >
{:else if settings.loading || !settings.profiles.data}
  <div role="status" aria-label={$t('common.loading')} class="grid gap-6">
    <Skeleton class="h-48" /><Skeleton class="h-96" />
  </div>
{:else if !settings.profiles.data.length}
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
    <TemplateCards clients={settings.clients.data ?? []} onchoose={newProfile} />
  </div>
{:else}
  <div class="grid grid-cols-[minmax(0,1fr)] items-start gap-6">
    <nav aria-label={$t('profiles.title')} class="space-y-1">
      {#each settings.profiles.data as profile (profile.profile_id)}
        <button
          type="button"
          class="flex w-full items-start gap-2.5 rounded-item px-3 py-2 text-left text-sm transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-60 {settings.selectedId ===
          profile.profile_id
            ? 'bg-muted'
            : ''}"
          aria-current={settings.selectedId === profile.profile_id ? 'true' : undefined}
          disabled={settings.saving}
          onclick={() => {
            settings.selectedId = profile.profile_id
            openEditor(profile.name)
          }}
        >
          <ClientIcon
            clientType={profile.client_type ?? undefined}
            acpAgentId={profile.client_key}
            class="mt-0.5 size-4 shrink-0 text-foreground"
          />
          <span class="min-w-0 flex-1">
            <span
              class="block truncate {settings.selectedId === profile.profile_id ? 'font-medium text-heading' : ''}"
              >{profile.name}</span
            >
            <span class="flex gap-1 text-xs text-muted-foreground">
              <span class="truncate">{profile.model ?? $t('profiles.noLaunch')}</span>
              {#if profile.use_yolo}<span class="shrink-0">· {$t('profiles.yoloMode')}</span>{/if}
            </span>
          </span>
        </button>
      {/each}
    </nav>
  </div>
{/if}
{#snippet editor()}
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
      oncancel={() => stack.back()}
      onsaved={() => {
        settings.profileSaved(null)
        stack.release(editorId)
      }}
      ondelete={(profile) => (settings.deleting = profile)}
    />
  {/key}
{/snippet}
<DeleteProfileDialog
  bind:profile={settings.deleting}
  ondeleted={(profile) => {
    if (settings.selectedId === profile.profile_id) stack.release(editorId)
  }}
/>
{#snippet add()}<Button
    variant="ghost"
    size="icon"
    aria-label={$t('profiles.newShort')}
    disabled={settings.creationDisabled}
    onclick={() => newProfile()}><PlusIcon /></Button
  >{/snippet}
