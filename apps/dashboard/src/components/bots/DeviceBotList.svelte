<script lang="ts">
  import { createQueries, createQuery, skipToken } from '@tanstack/svelte-query'
  import { page } from '$app/state'
  import { t } from '$lib/i18n'
  import { dashboardPath, navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { cn } from '$lib/utils.js'
  import BotAvatar from './BotAvatar.svelte'
  import { useDeviceRuntime } from '../../device/context'
  import BotSettingsDialog from './BotSettingsDialog.svelte'

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
        queryKey: ['bot-processing', bot.bot_id],
        queryFn: skipToken,
      })),
    }),
    () => device.queryClient,
  )
  const scope = $derived(handle ? `/${handle}` : undefined)
  let creating = $state(false)
  let botsOpen = $state(true)
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
          {@const working = !!bot.working_on || statuses[index]?.data === true}
          {@const path = `/bots/${encodeURIComponent(bot.bot_id)}`}
          {@const active = page.params.botId === bot.bot_id && (handle === undefined || handle === page.params.handle)}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton isActive={active} tooltipContent={bot.name} class="h-9 gap-2.5 [&_svg]:size-6 group-data-[collapsible=icon]:p-1!">
              {#snippet child({ props })}
                <a {...props} href={scope ? scope + path : dashboardPath(path)} aria-current={active ? 'page' : undefined}
                  onclick={(event) => { event.preventDefault(); void navigate(path, undefined, { scope }) }}>
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

{#if creating}
  <BotSettingsDialog onclose={() => { creating = false }} onsaved={(bot) => {
    creating = false
    sidebar.setOpenMobile(false)
    void navigate(`/bots/${encodeURIComponent(bot.bot_id)}`, undefined, { scope })
  }} />
{/if}
