<script lang="ts">
  import { untrack } from 'svelte'
  import { goto } from '$app/navigation'
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { dashboardPath, dashboardScope, navigate } from '$lib/navigation'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { readStored, scopedKey, writeStored } from '$lib/storage'
  import * as Dialog from '$lib/components/ui/dialog'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import ScrollIcon from 'phosphor-svelte/lib/ScrollIcon'
  import ChatCircleDotsIcon from 'phosphor-svelte/lib/ChatCircleDotsIcon'
  import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { useDeviceRuntime } from '../../device/context'
  import type { BotChatController } from '../../features/bots/botChat.svelte'
  import { createProfileQueries } from '../../features/settings/agentProfiles.svelte'
  import ProfileForm from '../../pages/agentProfiles/ProfileForm.svelte'
  import { profileTemplates } from '../../pages/agentProfiles/publishedTemplates'
  import { templateClient, templateIcon, type ProfileTemplate } from '../../pages/agentProfiles/templates'
  import ClientIcon from '../chat/ClientIcon.svelte'
  import type { AgentProfileView } from '../../api/types'

  let { chat, name }: { chat: BotChatController; name: string } = $props()
  const device = useDeviceRuntime()
  const path = '/settings/agent-profiles'
  const deviceKey = scopedKey('pontia.agentProfileHint.dismissed', device.id)
  const botKey = scopedKey('pontia.agentProfileHint.closed', device.id, untrack(() => chat.botId))
  const isHidden = () => !!(readStored(deviceKey) || readStored(botKey))
  let hidden = $state(isHidden())
  let selected = $state<ProfileTemplate>()
  const eligible = $derived(chat.timeline.status.kind === 'idle' && !chat.quote && !hidden)
  const settings = createProfileQueries(() => eligible || !!selected)
  const available = $derived($profileTemplates.filter(template => templateClient(template, settings.enabledClients)))
  const scope = dashboardScope()
  let trigger: HTMLButtonElement
  let formContainer: HTMLDivElement
  let asking = $state(false)
  function dismiss(key: string) {
    writeStored(key, '1')
    hidden = true
  }

  function syncDismissal(event: StorageEvent) {
    if (event.key === deviceKey || event.key === botKey || event.key === null) hidden = isHidden()
  }

  async function ask() {
    if (asking) return
    asking = true
    try {
      await chat.post($t('bots.profileHint.askMessage'), null)
      dismiss(botKey)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error), { duration: 6000 })
    } finally {
      asking = false
    }
  }
  function saved(profile: AgentProfileView) {
    selected = undefined
    toast.success($t('bots.profileHint.added', { profile: profile.name, name }), {
      duration: 6000,
      action: { label: $t('bots.profileHint.view'), onClick: () => void navigate(path, { profile: profile.profile_id }, { scope }) },
    })
  }
</script>

<svelte:window onstorage={syncDismissal} />

{#if eligible && settings.profiles.isSuccess && settings.profiles.data.length === 0 && settings.enabledClients.length}
  <section class:mobile-glass={isMobileShell()} aria-label={$t('bots.profileHint.title', { name })}
    class="profile-hint pointer-events-auto mb-2 grid grid-cols-[auto_1fr_auto_auto] items-center gap-x-2 gap-y-2 rounded-item border bg-card p-3 max-md:grid-cols-[auto_1fr_auto] max-md:rounded-item">
    <span class="grid size-8 place-items-center rounded-item bg-muted"><ScrollIcon class="size-4"/></span>
    <div class="min-w-0">
      <h2 class="text-sm font-semibold text-heading max-md:text-15">{$t('bots.profileHint.title', { name })}</h2>
      <p class="text-13 text-muted-foreground max-md:text-13">{$t(isMobileShell() ? 'bots.profileHint.descriptionShort' : 'bots.profileHint.description', { name })}</p>
    </div>
    <button class="flex items-center gap-1 text-xs font-medium text-primary max-md:col-span-2 max-md:col-start-1 max-md:row-start-3 max-md:min-h-11 max-md:justify-self-start" onclick={ask} disabled={asking}><ChatCircleDotsIcon class="size-4"/>{$t('bots.profileHint.askBot', { name })}</button>
    <Tooltip.Provider>
      <Tooltip.Root>
        <Tooltip.Trigger aria-label={$t('bots.profileHint.closeForBot', { name })} class="grid size-7 place-items-center self-start rounded-full text-muted-foreground hover:bg-muted max-md:col-start-3 max-md:row-start-1 max-md:size-11" onclick={() => dismiss(botKey)}><XIcon class="size-4"/></Tooltip.Trigger>
        {#if !isMobileShell()}<Tooltip.Content side="top">{$t('bots.profileHint.closeForBot', { name })}</Tooltip.Content>{/if}
      </Tooltip.Root>
    </Tooltip.Provider>
    <div class="hint-templates no-scrollbar col-span-3 flex min-w-0 items-center gap-1.5 overflow-x-auto md:col-span-4 md:col-start-1 md:row-start-2 md:mr-20">
      {#each available.slice(0, 3) as template (template.id)}
        {@const Icon = templateIcon(template)}
        {@const client = templateClient(template, settings.enabledClients)!}
        <button class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-chip border bg-background px-2.5 text-13 font-medium hover:border-primary max-md:min-h-11" onclick={(event) => { trigger = event.currentTarget; selected = template }}>
          <span class="grid size-5 place-items-center rounded-full bg-muted"><Icon class="size-3.5"/></span>{template.text.en.name}
          <span class="flex items-center gap-1 text-xs font-normal text-muted-foreground max-md:hidden"><ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-3.5"/>{client.name}</span>
        </button>
      {/each}
      <button class="inline-flex h-8 shrink-0 items-center gap-1 rounded-chip border border-dashed px-3 text-xs hover:bg-muted max-md:min-h-11" onclick={() => navigate(path)}>{$t('bots.profileHint.allTemplates')}<ArrowRightIcon class="size-3"/></button>
    </div>
    <button class="col-start-4 row-start-2 justify-self-end whitespace-nowrap rounded-item px-1.5 py-1 text-xs text-muted-foreground hover:bg-muted max-md:col-start-2 max-md:col-end-4 max-md:row-start-3 max-md:min-h-11" onclick={() => dismiss(deviceKey)}>{$t('bots.profileHint.dontShowAgain')}</button>
  </section>
{/if}

<Dialog.Root open={!!selected} onOpenChange={open => { if (!open) selected = undefined }}>
  <ResponsiveDialog mode="sheet" dismiss={() => selected = undefined} aria-describedby={undefined} class="sm:max-w-[480px]"
    onOpenAutoFocus={event => { event.preventDefault(); formContainer?.querySelector<HTMLButtonElement>('button[type="submit"]')?.focus({ preventScroll: true }) }}
    onCloseAutoFocus={event => { event.preventDefault(); trigger?.focus({ preventScroll: true }) }}>
    <Dialog.Title class="text-base font-semibold max-md:text-center">{$t('bots.profileHint.addTitle')}</Dialog.Title>
    <div bind:this={formContainer} class="grid min-h-0 min-w-0 gap-3 max-md:max-h-[75dvh] max-md:overflow-y-auto max-md:py-3">
      {#if selected}
        <ProfileForm quickTemplates={available} botName={name} template={selected} clients={settings.enabledClients} allClients={settings.clients.data ?? []}
          oncancel={() => selected = undefined} onsaved={saved}
          onedit={profileDraft => goto(dashboardPath(path), { state: { profileDraft, mobileBack: isMobileShell() } })} />
      {/if}
    </div>
  </ResponsiveDialog>
</Dialog.Root>

<style>
  @media (max-width: 767px) {
    .hint-templates { padding-right: 10px; mask-image: linear-gradient(to right, black calc(100% - 10px), transparent) }
    .profile-hint { --glass: var(--glass-strong) }
    :global(.bot-chat-dock:has([contenteditable="true"]:focus)) .profile-hint { display: none }
  }
</style>
