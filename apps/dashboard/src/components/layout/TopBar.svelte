<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { writable } from 'svelte/store'
  import { t, number } from '$lib/i18n';
  import { downloads } from '$lib/downloads/manager'
  import { useDeviceRuntime } from '../../device/context';
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import BotAvatar from '../bots/BotAvatar.svelte'
  import TopBarMoreMenu from './TopBarMoreMenu.svelte'
  import { navigate } from '$lib/navigation'
  import { returnToLastPage } from '$lib/lastPage'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import BotSettingsDialog from '../bots/BotSettingsDialog.svelte'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import SettingsSectionSheet from '../settings/SettingsSectionSheet.svelte'
  import { useSettingsDetail } from '../settings/settingsDetail'
  import { isSettingsPath, isSettingsSectionActive, settingsSections } from '../settings/sections'
  import { snapshotSessionOverview } from "../../queries/sessionOverview";
  const device = useDeviceRuntime();
  const { sessionDetail, sessionDetailError, updateSessionTitle } = device.sessions;
  const { createSessionOverviewQuery } = device.sessionOverview;

  let { onNewChat = () => void navigate('/') }: { onNewChat?: () => void } = $props()

  let currentPath = $state(dashboardRelativePath())
  const overviewQuery = createSessionOverviewQuery()
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))
  let renameDialogOpen = $state(false)
  let botSettingsOpen = $state(false)
  let renameBusy = $state(false)
  let renameError = $state<string | null>(null)

  function updatePath(): void {
    currentPath = dashboardRelativePath()
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isChatPath(path: string): boolean {
    return path === '/' || path === '/bot' || path.startsWith('/chat/') || path.startsWith('/bots/')
  }

  const sessionId = $derived(currentPath.startsWith('/chat/') ? decodeURIComponent(currentPath.split('/')[2] ?? '') : '')
  const botId = $derived(currentPath.startsWith('/bots/') ? decodeURIComponent(currentPath.split('/')[2] ?? '') : '')
  const bots = createQuery(
    () => ({
      queryKey: ['bots'],
      queryFn: ({ signal }) => device.api.listBots(signal),
      enabled: !!botId,
    }),
    () => device.queryClient,
  )
  const currentBot = $derived(bots.data?.find(bot => bot.bot_id === botId))
  const overviewSession = $derived(
    overview.pinned.find((item) => item.session_id === sessionId)
      ?? overview.active.find((item) => item.session_id === sessionId)
      ?? overview.list.find((item) => item.session_id === sessionId)
      ?? null
  )
  const session = $derived(sessionId
    ? ($sessionDetail?.session.session_id === sessionId ? $sessionDetail.session : ($sessionDetailError ? null : overviewSession))
    : null)
  const activeDownloadCount = $derived($downloads.filter(task => task.status === 'receiving').length)
  const title = $derived(session ? sessionChatTitle(session, $t) : '')
  const chatPath = $derived(isChatPath(currentPath))
  const settingsPath = $derived(isSettingsPath(currentPath))
  const settingsTitle = $derived($t(settingsSections.find((section) => isSettingsSectionActive(section, currentPath))?.label ?? 'settings.title'))
  const settingsDetail = useSettingsDetail() ?? writable(null)
  // Public Dashboards name the device whose settings are shown.
  const settingsEyebrow = $derived(device.name ? `${$t('settings.title')} · ${device.name}` : $t('settings.title'))
  let sectionSheetOpen = $state(false)
  const pillClass = 'pointer-events-auto h-12 rounded-lg bg-popover px-0.5 ring-1 ring-foreground/10 md:h-8 md:px-1'
  const groupClass = $derived(['flex min-w-0 items-center gap-1', chatPath && pillClass, chatPath && !title && !botId && 'md:hidden'])
  // Phones get larger touch targets; wider screens keep the compact bar.
  const mobileIconClass = 'size-11 md:size-7 [&_svg:not([class*=size-])]:size-5 md:[&_svg:not([class*=size-])]:size-4'

  // Opens the Bot chat or Session chat last viewed on this device, else New Chat.
  function backToChat(): void {
    returnToLastPage()
    onNewChat()
  }

  function openRenameDialog(): void {
    renameError = null
    renameDialogOpen = true
  }

  async function confirmRename(nextTitle: string | null): Promise<void> {
    if (!session) return
    renameBusy = true
    renameError = null
    try {
      await updateSessionTitle(session.session_id, nextTitle)
      renameDialogOpen = false
    } catch (error) {
      renameError = error instanceof Error ? error.message : String(error)
    } finally {
      renameBusy = false
    }
  }
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<!-- Phones leave a gap above the tall touch targets; wider screens keep the compact bar. -->
<header class={['flex h-14 items-center pt-2 md:pt-0', chatPath ? 'pointer-events-none px-3 md:h-12 md:px-4' : settingsPath ? 'bg-surface pr-3 md:h-12 md:pr-0' : 'bg-surface px-3 md:h-8 md:px-4']}>
  {#if settingsPath}
    <!-- Phones title the page here and switch sections from a bottom sheet; wider screens list them in the sidebar and show the current one as the title. -->
    <div class="flex min-w-0 items-center gap-1 px-2 md:hidden">
      {#if $settingsDetail}
        {@const detail = $settingsDetail}
        <Button variant="ghost" size="icon-sm" class={mobileIconClass} aria-label={settingsTitle} disabled={detail.disabled} onclick={detail.back}>
          <CaretLeftIcon />
        </Button>
        <div class="flex min-w-0 flex-col py-1 pl-1 leading-tight">
          <span class="truncate text-[0.6875rem] text-muted-foreground">{settingsTitle}</span>
          <span class="truncate text-base font-semibold text-heading">{detail.title}</span>
        </div>
      {:else}
        <Button variant="ghost" size="icon-sm" class={mobileIconClass} aria-label={$t('nav.backToChat')} onclick={backToChat}>
          <CaretLeftIcon />
        </Button>
        <button
          type="button"
          class="flex h-11 min-w-0 items-center gap-1.5 rounded-md px-2.5 text-left text-heading outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring aria-expanded:bg-muted"
          aria-haspopup="dialog"
          aria-expanded={sectionSheetOpen}
          title={$t('settings.switchSection')}
          onclick={() => { sectionSheetOpen = true }}
        >
          <span class="flex min-w-0 flex-col leading-tight">
            <span class="truncate text-[0.6875rem] text-muted-foreground">{settingsEyebrow}</span>
            <span class="truncate text-base font-semibold">{settingsTitle}</span>
          </span>
          <CaretDownIcon aria-hidden="true" class={['size-4 shrink-0 text-muted-foreground transition-transform', sectionSheetOpen && 'rotate-180']} />
        </button>
      {/if}
    </div>
    <h1 class="hidden px-6 text-base font-semibold text-heading md:block">{settingsTitle}</h1>
  {:else}
  {#if botId}
    <div class={['mr-2 flex shrink-0 items-center md:hidden', pillClass]}>
      <Sidebar.Trigger class={mobileIconClass} />
    </div>
  {/if}
  <div class={groupClass}>
    {#if !botId}<Sidebar.Trigger class={[mobileIconClass, 'md:hidden']} />{/if}
    {#if botId && currentBot}
      <h1 class="mx-1 min-w-0 max-w-80">
        <!-- Phones get a full-height touch target; wider screens keep the compact title. -->
        <button
          type="button"
          class="flex h-11 w-full min-w-0 items-center gap-1.5 rounded-md px-2 text-left text-sm font-normal text-heading focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none md:h-auto md:py-0.5"
          title={$t('bots.edit')}
          aria-haspopup="dialog"
          onclick={() => { botSettingsOpen = true }}
        >
          <BotAvatar shape={currentBot.avatar_shape} color={currentBot.avatar_color} class="size-5" />
          <span class="min-w-0 truncate">{currentBot.name}</span>
        </button>
      </h1>
    {:else if title}
      <h1 class="mx-1 hidden min-w-0 max-w-80 md:block">
        <button
          type="button"
          class="group flex w-full min-w-0 items-center gap-1.5 rounded-md px-2 py-0.5 text-left text-sm font-normal text-heading focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          title={$t('common.renameNamed', { name: title })}
          onclick={openRenameDialog}
        >
          <span class="min-w-0 truncate">{title}</span>
          <PencilSimpleIcon aria-hidden="true" class="hidden size-3.5 shrink-0 text-muted-foreground group-hover:block group-focus-visible:block" />
        </button>
      </h1>
    {/if}
  </div>
  {/if}
  {#if settingsPath}
    <TopBarMoreMenu />
  {:else if botId}
    <!-- Wider screens open downloads from the download center beside the top bar. -->
    {#if $downloads.length}
      <div class={['ml-auto shrink-0 md:hidden', pillClass]}>
        <Button variant="ghost" size="icon-sm" class="relative size-11" aria-label={$t('downloads.title')} onclick={() => document.dispatchEvent(new CustomEvent('pontia:open-downloads'))}>
          <DownloadSimpleIcon class={['size-5', activeDownloadCount && 'text-primary']} />
          <Badge data-testid="downloads-count" class="absolute top-0.5 right-0.5 h-4 min-w-4 rounded-full px-1 text-[0.625rem] tabular-nums">{$number($downloads.length)}</Badge>
        </Button>
      </div>
    {/if}
  {:else}
  <TopBarMoreMenu>
      <DropdownMenu.Item class="min-h-11 gap-3 px-3" onclick={onNewChat}>
        <NotePencilIcon /> {$t('nav.newChat')}
      </DropdownMenu.Item>
      {#if session}
        <DropdownMenu.Item class="min-h-11 gap-3 px-3" onclick={openRenameDialog}>
          <PencilSimpleIcon /> {$t('chat.rename')}
        </DropdownMenu.Item>
      {/if}
  </TopBarMoreMenu>
  {/if}
</header>

{#if settingsPath}
  <SettingsSectionSheet bind:open={sectionSheetOpen} {currentPath} deviceName={device.name} onBackToChat={backToChat} />
{/if}

{#if botSettingsOpen && botId && currentBot}
  <BotSettingsDialog bot={currentBot} onclose={() => { botSettingsOpen = false }} onsaved={() => {
    botSettingsOpen = false
  }} />
{/if}

<RenameSessionDialog
  bind:open={renameDialogOpen}
  {session}
  busy={renameBusy}
  error={renameError}
  onConfirm={(nextTitle) => void confirmRename(nextTitle)}
/>
