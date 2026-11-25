<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import { useDashboardHost } from '../../device/hostContext'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import DesktopFrame from './DesktopFrame.svelte'
  import DeviceSessionList from '../../components/layout/DeviceSessionList.svelte';
  import SidebarDeviceSwitcher from './SidebarDeviceSwitcher.svelte';
  import AppSidebar from '../../components/layout/AppSidebar.svelte';
  import DeviceBotList from '../../components/bots/DeviceBotList.svelte';
  import TopBar from '../../components/layout/TopBar.svelte';
  import ChatShortcuts from '../../components/chat/ChatShortcuts.svelte';
  import ConnectionNotice from '../../components/layout/ConnectionNotice.svelte';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import ShortcutManager from '$lib/components/shortcuts/ShortcutManager.svelte'
  import ShortcutHelp from '$lib/components/shortcuts/ShortcutHelp.svelte'
  import { registerShortcut } from '$lib/shortcuts/dispatch'
  let { children }: { children: Snippet } = $props()
  const host = useDashboardHost()
  if (host.mode === 'public') registerShortcut('previousDevice', event => {
    if (!event.repeat && host.previousDevice) host.selectDevice(host.previousDevice.handle)
  })
  const active = $derived(host.activeDevice)
  const device = $derived(active?.state === 'online' ? active.runtime : null)
</script>

<ShortcutManager />
<ShortcutHelp scope={host.routeScope} />
<DesktopFrame>
  {#snippet sidebar()}
    {#if host.mode === 'local'}
      <ChatShortcuts />
      <ConnectionNotice />
      <AppSidebar onNewChat={host.openNewChat} onSettings={host.openSettings} />
    {:else}
      <AppSidebar onNewChat={host.openNewChat}
        settingsDisabled={!host.routeScope} settingsScope={host.routeScope}
        onSettings={host.openSettings}>
        {#snippet deviceSwitcher()}
          {#if host.canSwitchDevice}
            <SidebarDeviceSwitcher devices={host.devices} selected={active} previous={host.previousDevice} onSelect={host.selectDevice} />
          {/if}
        {/snippet}
        {#snippet groups()}
          {#if active?.state === 'online' && active.runtime}
            {#key active.runtime}
              <DeviceProvider device={active.runtime}>
                <DeviceBotList handle={active.handle} />
                <DeviceSessionList handle={active.handle} onNewChat={host.openNewChat} />
              </DeviceProvider>
            {/key}
          {:else if active?.state === 'offline'}
            <div class="flex items-center justify-between gap-2 px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
              <span>{active.name} {$t('devices.offlineSuffix')}</span>
              <Button variant="outline" size="xs" onclick={() => host.retryDevice(active.handle)}>{$t('common.retry')}</Button>
            </div>
          {:else if host.connecting}
            <Sidebar.Group class="group-data-[collapsible=icon]:hidden">
              {#if !host.loadingDevices}<Sidebar.GroupLabel role="status">{$t('connection.connecting')}</Sidebar.GroupLabel>{/if}
              <Sidebar.GroupContent>
                <Sidebar.MenuSkeleton />
                <Sidebar.MenuSkeleton />
                <Sidebar.MenuSkeleton />
              </Sidebar.GroupContent>
            </Sidebar.Group>
          {/if}
        {/snippet}
      </AppSidebar>
      {#if device}
        {#key device}<DeviceProvider {device}><ChatShortcuts newChatHandle={active?.handle} /><ConnectionNotice /></DeviceProvider>{/key}
      {/if}
    {/if}
  {/snippet}
  {#snippet topbar()}
    {#if host.mode === 'local'}
      <TopBar />
    {:else if device}
      {#key device}<DeviceProvider {device}><TopBar /></DeviceProvider>{/key}
    {:else}
      <header class="h-12"></header>
    {/if}
  {/snippet}
  {#snippet children()}{@render children()}{/snippet}
</DesktopFrame>
