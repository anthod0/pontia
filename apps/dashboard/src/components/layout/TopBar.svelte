<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import GearIcon from 'phosphor-svelte/lib/GearIcon'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import BotAvatar from '../bots/BotAvatar.svelte'
  import TopBarMoreMenu from './TopBarMoreMenu.svelte'
  import { navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import BotSettingsDialog from '../bots/BotSettingsDialog.svelte'
  import SettingsSectionNav from '../settings/SettingsSectionNav.svelte'
  import { isSettingsPath, isSettingsSectionActive, settingsSections } from '../settings/sections'
  import { snapshotSessionOverview } from "../../queries/sessionOverview";
  const device = useDeviceRuntime();
  const sidebar = Sidebar.useSidebar();
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
  const botChat = createQuery(
    () => ({
      queryKey: ['bot-chat', botId],
      queryFn: ({ signal }) => device.api.getBotChat(botId, signal),
      enabled: !!botId,
    }),
    () => device.queryClient,
  )
  const overviewSession = $derived(
    overview.pinned.find((item) => item.session_id === sessionId)
      ?? overview.active.find((item) => item.session_id === sessionId)
      ?? overview.list.find((item) => item.session_id === sessionId)
      ?? null
  )
  const session = $derived(sessionId
    ? ($sessionDetail?.session.session_id === sessionId ? $sessionDetail.session : ($sessionDetailError ? null : overviewSession))
    : null)
  const title = $derived(session ? sessionChatTitle(session, $t) : '')
  const chatPath = $derived(isChatPath(currentPath))
  const settingsPath = $derived(isSettingsPath(currentPath))
  const settingsTitle = $derived($t(settingsSections.find((section) => isSettingsSectionActive(section, currentPath))?.label ?? 'settings.title'))
  const pillClass = 'pointer-events-auto h-12 rounded-lg bg-popover px-0.5 ring-1 ring-foreground/10 md:h-8 md:px-1'
  const groupClass = $derived(['flex min-w-0 items-center gap-1', chatPath && pillClass, chatPath && !title && !botId && 'md:hidden'])
  // Phones get larger touch targets; wider screens keep the compact bar.
  const mobileIconClass = 'size-11 md:size-7 [&_svg:not([class*=size-])]:size-5 md:[&_svg:not([class*=size-])]:size-4'

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

<header class={['flex items-center', chatPath ? 'pointer-events-none h-12 px-3 md:px-4' : settingsPath ? 'h-12 bg-surface pr-3 md:pr-0' : 'h-8 bg-surface px-3 md:px-4']}>
  {#if settingsPath}
    <!-- Phones switch sections here; wider screens list them in the sidebar and show the current one as the title. -->
    <div class="flex min-w-0 items-center gap-2 px-3 md:hidden">
      <Sidebar.Trigger class={mobileIconClass} />
      <SettingsSectionNav />
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
    {#if botId && botChat.data}
      <h1 class="mx-1 min-w-0 max-w-80">
        {#if sidebar.isMobile}
          <span class="flex min-w-0 items-center gap-1.5 px-2 text-sm font-normal text-heading">
            <BotAvatar shape={botChat.data.bot.avatar_shape} color={botChat.data.bot.avatar_color} class="size-5" />
            <span class="truncate">{botChat.data.bot.name}</span>
          </span>
        {:else}
          <button
            type="button"
            class="flex w-full min-w-0 items-center gap-1.5 rounded-md px-2 py-0.5 text-left text-sm font-normal text-heading focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            title={$t('bots.edit')}
            aria-haspopup="dialog"
            onclick={() => { botSettingsOpen = true }}
          >
            <BotAvatar shape={botChat.data.bot.avatar_shape} color={botChat.data.bot.avatar_color} class="size-5" />
            <span class="min-w-0 truncate">{botChat.data.bot.name}</span>
          </button>
        {/if}
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
      {#if botId && botChat.data}
        <DropdownMenu.Item class="min-h-11 gap-3 px-3" onclick={() => { botSettingsOpen = true }}>
          <GearIcon /> {$t('bots.edit')}
        </DropdownMenu.Item>
      {/if}
  </TopBarMoreMenu>
  {/if}
</header>

{#if botSettingsOpen && botId && botChat.data}
  <BotSettingsDialog bot={botChat.data.bot} onclose={() => { botSettingsOpen = false }} onsaved={() => {
    botSettingsOpen = false
    void device.queryClient.invalidateQueries({ queryKey: ['bot-chat', botId] })
  }} />
{/if}

<RenameSessionDialog
  bind:open={renameDialogOpen}
  {session}
  busy={renameBusy}
  error={renameError}
  onConfirm={(nextTitle) => void confirmRename(nextTitle)}
/>
