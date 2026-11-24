<script lang="ts">
  import ResponsiveMenu from '$lib/components/ui/overlay/ResponsiveMenu.svelte'
  import { createBotDirectory } from '../../features/bots/botDirectory.svelte'
  import { toast } from 'svelte-sonner'
  import { page } from '$app/state'
  import { t } from '$lib/i18n'
  import { dashboardPath, navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { openContextMenu, type MenuAnchor } from '$lib/contextMenu'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import BotAvatar from './BotAvatar.svelte'
  import { useDeviceRuntime } from '../../device/context'
  import BotSettingsDialog from './BotSettingsDialog.svelte'
  import BotDeleteDialog from './BotDeleteDialog.svelte'
  import { ApiError } from '../../api/errors'
  import type { Bot } from '../../api/types'

  let { handle }: { handle?: string } = $props()
  const directory = createBotDirectory(useDeviceRuntime())
  const sidebar = Sidebar.useSidebar()
  const scope = $derived(handle ? `/${handle}` : undefined)
  let creating = $state(false)
  let editing = $state<Bot | null>(null)
  let deleting = $state<Bot | null>(null)
  let menu = $state<{ bot: Bot; anchor: MenuAnchor; row: HTMLElement } | null>(null)
  let menuOpen = $state(false)

  const rail = $derived(sidebar.state === 'collapsed')

  function showMenu(event: MouseEvent, bot: Bot) {
    const anchor = openContextMenu(event)
    if (!anchor) return
    menu = { bot, anchor, row: event.currentTarget as HTMLElement }
    menuOpen = true
  }

  // Like the /reload command in the Bot chat: the next message starts a new Bot Session generation.
  async function reload(bot: Bot) {
    try {
      await directory.reload(bot.bot_id)
      toast.success($t('bots.reloadReady'))
    } catch (error) {
      toast.error(error instanceof ApiError && error.code === 'state_conflict'
        ? $t('bots.reloadBusy')
        : error instanceof Error ? error.message : String(error))
    }
  }
</script>

<Sidebar.Group>
  <Sidebar.GroupLabel class="sticky top-0 z-10 flex h-7 items-center gap-1 bg-sidebar p-0 pr-0.5 pl-2 max-md:h-9 group-data-[collapsible=icon]:hidden">
    <span class="text-[12.5px] font-semibold text-heading max-md:text-sm">{$t('bots.title')}</span>
    <span class="flex-1"></span>
    <button
      type="button"
      class="flex size-6 items-center justify-center rounded-full text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden max-md:size-9 [&>svg]:size-[15px] max-md:[&>svg]:size-[18px]"
      aria-label={$t('bots.create')}
      title={$t('bots.create')}
      onclick={() => { creating = true }}
    >
      <PlusIcon />
    </button>
  </Sidebar.GroupLabel>
  <Sidebar.GroupContent>
    {#if directory.bots?.length === 0}
      {#if rail}
        <Sidebar.Menu>
          <Sidebar.MenuItem>
            <Sidebar.MenuButton tooltipContent={$t('bots.create')} onclick={() => { creating = true }}>
              <PlusIcon /><span>{$t('bots.create')}</span>
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        </Sidebar.Menu>
      {:else}
        <div class="mt-1 flex flex-col gap-2.5 rounded-lg border border-dashed p-3.5">
          <p class="text-sm font-semibold text-heading">{$t('bots.setupFirst')}</p>
          <p class="text-[12.5px] leading-normal text-muted-foreground">{$t('bots.sidebarSetupDescription')}</p>
          <Button class="max-md:h-11" onclick={() => { creating = true }}><PlusIcon />{$t('bots.create')}</Button>
        </div>
      {/if}
    {/if}
    <Sidebar.Menu>
      {#each directory.bots ?? [] as bot, index (bot.bot_id)}
        {@const summary = directory.summaries[index]}
        {@const status = summary?.status}
        {@const working = status?.kind === 'working' || status?.kind === 'processing'}
        {@const todos = summary?.open_todo_count ?? 0}
        {@const path = `/bots/${encodeURIComponent(bot.bot_id)}`}
        {@const active = page.params.botId === bot.bot_id && (handle === undefined || handle === page.params.handle)}
        <Sidebar.MenuItem>
          <Sidebar.MenuButton isActive={active} tooltipContent={bot.name}
            class="group-has-data-[sidebar=menu-action]/menu-item:pr-3.5 touch-target:pr-12! h-auto min-h-12 gap-2.5 px-2 py-1.5 max-md:min-h-15 max-md:gap-3 max-md:px-2.5 max-md:py-2 [&_svg]:size-7 max-md:[&_svg]:size-9 group-data-[collapsible=icon]:min-h-0 group-data-[collapsible=icon]:p-1! group-data-[collapsible=icon]:[&_svg]:size-6">
            {#snippet child({ props })}
              <a {...props} href={scope ? scope + path : dashboardPath(path)} aria-current={active ? 'page' : undefined}
                onclick={(event) => { event.preventDefault(); void navigate(path, undefined, { scope }) }}
                oncontextmenu={(event) => showMenu(event, bot)}>
                <span class="relative shrink-0">
                  <BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} {working} />
                  {#if todos > 0}
                    <span role="img" aria-label={$t('bots.openTodos', { count: todos })}
                      class="pointer-events-none absolute -top-[3px] -right-[3px] size-2 rounded-full bg-primary ring-2 ring-sidebar transition-[box-shadow] duration-[80ms] group-hover/menu-button:ring-sidebar-accent group-data-active/menu-button:ring-sidebar-accent max-md:size-2.5"></span>
                  {/if}
                </span>
                <span class="flex min-w-0 flex-1 flex-col gap-px group-data-[collapsible=icon]:sr-only">
                  <span class="truncate font-medium text-heading group-data-[active=true]/menu-button:font-semibold max-md:text-base">{bot.name}</span>
                  {#if working}
                    <span class="truncate text-xs text-primary max-md:text-[13px]">{status.kind === 'working' ? status.title : $t('bots.processing')}</span>
                  {/if}
                </span>
              </a>
            {/snippet}
          </Sidebar.MenuButton>
          <Sidebar.MenuAction class="hidden touch-target:flex touch-target:size-11 touch-target:right-0 touch-target:after:hidden"
            aria-label={`${bot.name} · ${$t('bots.actions')}`} aria-haspopup="menu" aria-expanded={menuOpen && menu?.bot.bot_id === bot.bot_id}
            onclick={event => {
              const row = event.currentTarget as HTMLElement
              menu = { bot, row, anchor: row }
              menuOpen = true
            }}>
            <DotsThreeVerticalIcon />
          </Sidebar.MenuAction>
        </Sidebar.MenuItem>
      {/each}
    </Sidebar.Menu>
  </Sidebar.GroupContent>
</Sidebar.Group>

<ResponsiveMenu bind:open={menuOpen} label={menu?.bot.name ?? $t('bots.actions')} aria-label={$t('bots.actions')} customAnchor={menu?.anchor} side="bottom" align="start" sideOffset={2} class="w-auto min-w-44"
    onCloseAutoFocus={(event) => { event.preventDefault(); if (!editing && !deleting && menu?.row.isConnected) menu.row.focus() }}>
  {#if menu}
    {@const bot = menu.bot}
    <DropdownMenu.Item onSelect={() => { editing = bot }}><PencilSimpleIcon />{$t('bots.edit')}</DropdownMenu.Item>
    <DropdownMenu.Item disabled={directory.reloading === bot.bot_id} onSelect={() => void reload(bot)}><ArrowClockwiseIcon />{$t('bots.reload')}</DropdownMenu.Item>
    <DropdownMenu.Separator />
    <DropdownMenu.Item variant="destructive" onSelect={() => { deleting = bot }}><TrashIcon />{$t('bots.delete')}</DropdownMenu.Item>
  {/if}
</ResponsiveMenu>

{#if editing}
  <BotSettingsDialog bot={editing} onclose={() => { editing = null }} onsaved={() => { editing = null }} />
{/if}

{#if deleting}
  <BotDeleteDialog bot={deleting} onclose={() => { deleting = null }} ondone={() => { deleting = null }} />
{/if}

{#if creating}
  <BotSettingsDialog onclose={() => { creating = false }} onsaved={(bot) => {
    creating = false
    void navigate(`/bots/${encodeURIComponent(bot.bot_id)}`, undefined, { scope })
  }} />
{/if}
