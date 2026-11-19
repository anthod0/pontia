<script lang="ts">
  import { createQueries, createQuery } from '@tanstack/svelte-query'
  import { page } from '$app/state'
  import { t } from '$lib/i18n'
  import { dashboardPath, navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
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
  // Read the chats the Dashboard already keeps live without fetching them here; bots without
  // a loaded chat fall back to the work description from the bot list.
  const chats = createQueries(
    () => ({
      queries: (bots.data ?? []).map((bot) => ({
        queryKey: ['bot-chat', bot.bot_id],
        queryFn: ({ signal }: { signal: AbortSignal }) => device.api.getBotChat(bot.bot_id, signal),
        enabled: false,
      })),
    }),
    () => device.queryClient,
  )
  const scope = $derived(handle ? `/${handle}` : undefined)
  let creating = $state(false)
</script>

<Sidebar.Group class="shrink-0">
  <Sidebar.GroupLabel>{$t('bots.title')}</Sidebar.GroupLabel>
  <Sidebar.GroupContent>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <Sidebar.MenuButton tooltipContent={$t('bots.create')} onclick={() => { creating = true }}>
          <PlusIcon /><span>{$t('bots.create')}</span>
        </Sidebar.MenuButton>
      </Sidebar.MenuItem>
      {#each bots.data ?? [] as bot, index (bot.bot_id)}
        {@const chat = chats[index]?.data}
        {@const working = chat ? !!chat.bot.working_on || chat.processing : !!bot.working_on}
        {@const path = `/bots/${encodeURIComponent(bot.bot_id)}`}
        {@const active = page.params.botId === bot.bot_id && (handle === undefined || handle === page.params.handle)}
        <Sidebar.MenuItem>
          <Sidebar.MenuButton isActive={active} tooltipContent={bot.name} class="[&_svg]:size-5">
            {#snippet child({ props })}
              <a {...props} href={scope ? scope + path : dashboardPath(path)} aria-current={active ? 'page' : undefined}
                onclick={(event) => { event.preventDefault(); void navigate(path, undefined, { scope }) }}>
                <BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} {working} class="size-5" /><span>{bot.name}</span>
              </a>
            {/snippet}
          </Sidebar.MenuButton>
        </Sidebar.MenuItem>
      {/each}
    </Sidebar.Menu>
  </Sidebar.GroupContent>
</Sidebar.Group>

{#if creating}
  <BotSettingsDialog onclose={() => { creating = false }} onsaved={(bot) => {
    creating = false
    sidebar.setOpenMobile(false)
    void navigate(`/bots/${encodeURIComponent(bot.bot_id)}`, undefined, { scope })
  }} />
{/if}
