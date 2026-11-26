<script lang="ts">
  import { t } from "$lib/i18n"
  import { createQuery } from '@tanstack/svelte-query'
  import { useDeviceRuntime } from '../../device/context';
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import BotAvatar from '../bots/BotAvatar.svelte'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import BotSettingsDialog from '../bots/BotSettingsDialog.svelte'
  import { isSettingsPath, isSettingsSectionActive, settingsSections } from '../settings/sections'
  import { snapshotSessionOverview } from "../../queries/sessionOverview";
  const device = useDeviceRuntime();
  const { sessionDetail, sessionDetailError, updateSessionTitle } = device.sessions;
  const { createSessionOverviewQuery } = device.sessionOverview;

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
    return path === '/new' || path === '/' || path === '/bot' || path.startsWith('/chat/') || path.startsWith('/bots/')
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
  const title = $derived(session ? sessionChatTitle(session, $t) : '')
  const chatPath = $derived(isChatPath(currentPath))
  const settingsPath = $derived(isSettingsPath(currentPath))
  const settingsTitle = $derived($t(settingsSections.find((section) => isSettingsSectionActive(section, currentPath))?.label ?? 'settings.title'))
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

<header class={['flex items-center', chatPath ? 'pointer-events-none h-12 px-4' : settingsPath ? 'bg-surface h-12' : 'bg-surface h-8 px-4']}>
  {#if settingsPath}
    <h1 class="px-6 text-base font-semibold text-heading">{settingsTitle}</h1>
  {:else if title || currentBot}
    <div class={['flex min-w-0 items-center gap-1', chatPath && 'pointer-events-auto h-8 rounded-chip bg-popover px-1 border border-border']}>
    {#if botId && currentBot}
      <h1 class="mx-1 min-w-0 max-w-80">
        <button
          type="button"
          class="flex w-full min-w-0 items-center gap-1.5 rounded-chip px-2 text-left text-sm font-normal text-heading focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none py-0.5"
          title={$t('bots.edit')}
          aria-haspopup="dialog"
          onclick={() => { botSettingsOpen = true }}
        >
          <BotAvatar shape={currentBot.avatar_shape} color={currentBot.avatar_color} class="size-5" />
          <span class="min-w-0 truncate">{currentBot.name}</span>
        </button>
      </h1>
    {:else if title}
      <h1 class="mx-1 min-w-0 max-w-80">
        <button
          type="button"
          class="group flex w-full min-w-0 items-center gap-1.5 rounded-chip px-2 py-0.5 text-left text-sm font-normal text-heading focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
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
</header>

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
