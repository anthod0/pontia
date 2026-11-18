<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { page } from '$app/state'
  import { t } from '$lib/i18n'
  import { dashboardPath, navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import { useDeviceRuntime } from '../../device/context'
  import BotSettingsDialog from './BotSettingsDialog.svelte'

  let { handle }: { handle?: string } = $props()
  const device = useDeviceRuntime()
  const sidebar = Sidebar.useSidebar()
  const bots = createQuery(
    () => ({ queryKey: ['bots'], queryFn: ({ signal }) => device.api.listBots(signal) }),
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
      {#each bots.data ?? [] as bot (bot.bot_id)}
        {@const path = `/bots/${encodeURIComponent(bot.bot_id)}`}
        {@const active = page.params.botId === bot.bot_id && (handle === undefined || handle === page.params.handle)}
        <Sidebar.MenuItem>
          <Sidebar.MenuButton isActive={active} tooltipContent={bot.name}>
            {#snippet child({ props })}
              <a {...props} href={scope ? scope + path : dashboardPath(path)} aria-current={active ? 'page' : undefined}
                onclick={(event) => { event.preventDefault(); void navigate(path, undefined, { scope }) }}>
                <RobotIcon /><span>{bot.name}</span>
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
