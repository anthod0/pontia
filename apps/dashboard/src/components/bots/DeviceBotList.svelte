<script lang="ts">
  import { createQueries, createQuery } from '@tanstack/svelte-query'
  import { toast } from 'svelte-sonner'
  import { openBotChatStream } from '../../services/botChatStream'
  import { page } from '$app/state'
  import { t } from '$lib/i18n'
  import { dashboardPath, navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { openContextMenu, type MenuAnchor } from '$lib/contextMenu'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { cn } from '$lib/utils.js'
  import BotAvatar from './BotAvatar.svelte'
  import { useDeviceRuntime } from '../../device/context'
  import BotSettingsDialog from './BotSettingsDialog.svelte'
  import BotDeleteDialog from './BotDeleteDialog.svelte'
  import { ApiError } from '../../api/errors'
  import type { Bot } from '../../api/types'

  let { handle }: { handle?: string } = $props()
  const device = useDeviceRuntime()
  const sidebar = Sidebar.useSidebar()
  const bots = createQuery(
    () => ({ queryKey: ['bots'], queryFn: ({ signal }) => device.api.listBots(signal) }),
    () => device.queryClient,
  )
  const statuses = createQueries(
    () => ({
      queries: (bots.data ?? []).map((bot) => ({
        queryKey: ['bot-status', bot.bot_id],
        queryFn: async ({ signal }: { signal: AbortSignal }) => (await device.api.getBotMessages(bot.bot_id, { limit: 1 }, signal)).status,
      })),
    }),
    () => device.queryClient,
  )
  const botIds = $derived((bots.data ?? []).map(bot => bot.bot_id).join(","))
  $effect(() => {
    const close = botIds.split(',').filter(Boolean).map(id => openBotChatStream(device, id, update => {
      if (update.type === 'typing') return
      const query = { queryKey: ['bot-status', id] }
      // Invalidation alone can reuse an initial request that predates this update.
      void device.queryClient.cancelQueries(query).then(() => device.queryClient.invalidateQueries(query))
    }, () => {}))
    return () => close.forEach(stop => stop())
  })
  const scope = $derived(handle ? `/${handle}` : undefined)
  let creating = $state(false)
  let botsOpen = $state(true)
  let editing = $state<Bot | null>(null)
  let deleting = $state<Bot | null>(null)
  let reloading = $state<string | null>(null)
  let menu = $state<{ bot: Bot; anchor: MenuAnchor; row: HTMLElement } | null>(null)
  let menuOpen = $state(false)

  function showMenu(event: MouseEvent, bot: Bot) {
    const anchor = openContextMenu(event)
    if (!anchor) return
    menu = { bot, anchor, row: event.currentTarget as HTMLElement }
    menuOpen = true
  }

  // Like the /reload command in the Bot chat: the next message starts a new Bot Session generation.
  async function reload(bot: Bot) {
    reloading = bot.bot_id
    try {
      await device.api.reloadBot(bot.bot_id)
      toast.success($t('bots.reloadReady'))
    } catch (error) {
      toast.error(error instanceof ApiError && error.code === 'state_conflict'
        ? $t('bots.reloadBusy')
        : error instanceof Error ? error.message : String(error))
    } finally {
      reloading = null
    }
  }
</script>

<Sidebar.Group>
  <Sidebar.GroupLabel class="sticky top-0 z-10 flex h-8 items-center gap-1 bg-sidebar p-0 px-2 group-data-[collapsible=icon]:invisible group-data-[collapsible=icon]:static">
    <button
      type="button"
      class="flex min-w-0 items-center gap-1 rounded-sm text-left text-xs font-medium hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
      aria-expanded={botsOpen}
      onclick={() => (botsOpen = !botsOpen)}
    >
      <span>{$t('bots.title')}</span>
      <CaretRightIcon class={cn('size-3 transition-transform', botsOpen && 'rotate-90')} />
    </button>
    {#if bots.data?.length}
      <span class="flex-1"></span>
      <button
        type="button"
        class="rounded-sm hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
        aria-label={$t('bots.create')}
        title={$t('bots.create')}
        onclick={() => { creating = true }}
      >
        <PlusIcon class="size-4" />
      </button>
    {/if}
  </Sidebar.GroupLabel>
  <!-- The icon rail has no group labels, so it always shows the avatars. -->
  {#if botsOpen || (sidebar.state === 'collapsed' && !sidebar.isMobile)}
    <Sidebar.GroupContent>
      <Sidebar.Menu>
        {#if bots.data?.length === 0}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton tooltipContent={$t('bots.create')} onclick={() => { creating = true }}>
              <PlusIcon /><span>{$t('bots.create')}</span>
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        {/if}
        {#each bots.data ?? [] as bot, index (bot.bot_id)}
          {@const working = statuses[index]?.data?.kind === 'processing' || statuses[index]?.data?.kind === 'working'}
          {@const path = `/bots/${encodeURIComponent(bot.bot_id)}`}
          {@const active = page.params.botId === bot.bot_id && (handle === undefined || handle === page.params.handle)}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton isActive={active} tooltipContent={bot.name} class="h-9 gap-2.5 [&_svg]:size-6 group-data-[collapsible=icon]:p-1!">
              {#snippet child({ props })}
                <a {...props} href={scope ? scope + path : dashboardPath(path)} aria-current={active ? 'page' : undefined}
                  onclick={(event) => { event.preventDefault(); void navigate(path, undefined, { scope }) }}
                  oncontextmenu={(event) => showMenu(event, bot)}>
                  <BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} {working} class="size-6" /><span>{bot.name}</span>
                </a>
              {/snippet}
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        {/each}
      </Sidebar.Menu>
    </Sidebar.GroupContent>
  {/if}
</Sidebar.Group>

<DropdownMenu.Root bind:open={menuOpen}>
  <DropdownMenu.Content customAnchor={menu?.anchor} side="bottom" align="start" sideOffset={2} class="w-auto min-w-44" aria-label={$t('bots.actions')}
    onCloseAutoFocus={(event) => { event.preventDefault(); if (!editing && !deleting && menu?.row.isConnected) menu.row.focus() }}>
    {#if menu}
      {@const bot = menu.bot}
      <DropdownMenu.Item onSelect={() => { editing = bot }}><PencilSimpleIcon />{$t('bots.edit')}</DropdownMenu.Item>
      <DropdownMenu.Item disabled={reloading === bot.bot_id} onSelect={() => void reload(bot)}><ArrowClockwiseIcon />{$t('bots.reload')}</DropdownMenu.Item>
      <DropdownMenu.Separator />
      <DropdownMenu.Item variant="destructive" onSelect={() => { deleting = bot }}><TrashIcon />{$t('bots.delete')}</DropdownMenu.Item>
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>

{#if editing}
  <BotSettingsDialog bot={editing} onclose={() => { editing = null }} onsaved={() => { editing = null }} />
{/if}

{#if deleting}
  <BotDeleteDialog bot={deleting} onclose={() => { deleting = null }} ondone={() => { deleting = null }} />
{/if}

{#if creating}
  <BotSettingsDialog onclose={() => { creating = false }} onsaved={(bot) => {
    creating = false
    sidebar.setOpenMobile(false)
    void navigate(`/bots/${encodeURIComponent(bot.bot_id)}`, undefined, { scope })
  }} />
{/if}
