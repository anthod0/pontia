<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { writable } from 'svelte/store';
  import PublicNewChatPage from './PublicNewChatPage.svelte';
  import DeviceSessionList from '../../components/layout/DeviceSessionList.svelte';
  import SidebarDeviceSwitcher from './SidebarDeviceSwitcher.svelte';
  import { provideDeviceDirectory } from './deviceDirectory';
  import { Button } from '$lib/components/ui/button/index.js';
  import { navigate } from '$lib/navigation';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import AppSidebar from '../../components/layout/AppSidebar.svelte';
  import TopBar from '../../components/layout/TopBar.svelte';
  import ChatShortcuts from '../../components/chat/ChatShortcuts.svelte';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import DeviceProvider from '../../device/DeviceProvider.svelte';
  import { createDeviceConnections, type ConnectedDevice } from './deviceConnections';
  import { loadedRemoteAccess, loadRemoteAccess } from './remoteAccessLoader.svelte';

  let { handle, children }: { handle?: string; children: Snippet } = $props();
  let connections = $state.raw<ReturnType<typeof createDeviceConnections> | null>(null);
  const emptyDevices = writable<ConnectedDevice[]>([]);
  const initialLoading = writable(true);
  const noError = writable<string | null>(null);
  const signedIn = writable(false);
  const devices = $derived(connections?.devices ?? emptyDevices);
  const loading = $derived(connections?.loading ?? initialLoading);
  const error = $derived(connections?.error ?? noError);
  const signInRequired = $derived(connections?.signInRequired ?? signedIn);
  const selected = $derived($devices.find((device) => device.handle === handle));
  const device = $derived(selected?.state === 'online' ? selected.runtime : null);
  const remote = $derived(loadedRemoteAccess());
  let moduleError = $state<string | null>(null);
  let disposed = false;
  let loadingModule = false;
  const signInUrl = $derived($signInRequired ? remote?.dashboardSignInUrl(
    `${window.location.pathname}${window.location.search}${window.location.hash}`,
  ) : undefined);
  const dashboardState = $derived(selected?.state === 'online' ? 'available'
    : $signInRequired || selected?.state === 'offline' || moduleError || (!$loading && !selected)
      ? 'unavailable' : 'connecting');

  provideDeviceDirectory({ get devices() { return $devices }, get handle() { return handle }, rememberDevice });
  let lastHandle = $state<string | null>(null);
  const settingsHandle = $derived(handle || ($devices.some(target => target.handle === lastHandle)
    ? lastHandle : $devices.find(target => target.state === 'online')?.handle));
  // The sidebar follows the routed device; picking another device overrides it until the route changes.
  let sidebarChoice = $state<string | null>(null);
  $effect.pre(() => {
    void handle;
    sidebarChoice = null;
  });
  const sidebarHandle = $derived(
    (sidebarChoice && $devices.some(target => target.handle === sidebarChoice) ? sidebarChoice : undefined)
      ?? settingsHandle ?? $devices[0]?.handle);
  const sidebarDevice = $derived($devices.find(target => target.handle === sidebarHandle));

  function selectSidebarDevice(target: string): void {
    sidebarChoice = target;
    rememberDevice(target);
  }

  $effect(() => {
    if (handle && $devices.some(target => target.handle === handle)) {
      rememberDevice(handle);
    }
  });

  function rememberDevice(handle: string): void {
    lastHandle = handle;
    try { window.localStorage.setItem('pontia.lastDeviceHandle', handle); } catch { /* Storage is optional. */ }
  }

  async function start(): Promise<void> {
    if (loadingModule) return;
    loadingModule = true;
    moduleError = null;
    try {
      const access = await loadRemoteAccess();
      if (disposed) return;
      connections = createDeviceConnections(access);
      await connections.refreshDevices();
    } catch {
      if (!disposed) moduleError = 'Could not load remote device access.';
    } finally {
      loadingModule = false;
    }
  }

  function retry(): void {
    if (!connections) void start();
    else if (selected) connections.retry(selected.handle);
    else void connections.refreshDevices();
  }

  onMount(() => {
    try { lastHandle = window.localStorage.getItem('pontia.lastDeviceHandle'); } catch { /* Storage is optional. */ }
    void start();
    return () => {
      disposed = true;
      connections?.dispose();
    };
  });
</script>

<svelte:window onfocus={() => connections?.focus()} ononline={() => connections?.focus()} />
<svelte:document onvisibilitychange={() => {
  if (document.visibilityState === 'visible') connections?.focus();
}} />

<AppShell>
  {#snippet sidebar()}
    <AppSidebar onNewChat={() => void navigate('/', undefined, { scope: '' })}
      settingsDisabled={!settingsHandle}
      onSettings={() => { if (settingsHandle) void navigate('/settings/common', undefined, { scope: `/${settingsHandle}` }) }}>
      {#snippet groups()}
        {#if $devices.length}
          <SidebarDeviceSwitcher devices={$devices} selected={sidebarDevice} onSelect={selectSidebarDevice} />
        {/if}
        {#if sidebarDevice?.state === 'online' && sidebarDevice.runtime}
          {#key sidebarDevice.runtime}
            <DeviceProvider device={sidebarDevice.runtime}><DeviceSessionList handle={sidebarDevice.handle} /></DeviceProvider>
          {/key}
        {:else if sidebarDevice?.state === 'offline'}
          <div class="flex items-center justify-between gap-2 px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
            <span>{sidebarDevice.name} is offline.</span>
            <Button variant="outline" size="xs" onclick={() => connections?.retry(sidebarDevice.handle)}>Retry</Button>
          </div>
        {:else if sidebarDevice?.state === 'connecting'}
          <p role="status" class="px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">Connecting…</p>
        {/if}
      {/snippet}
    </AppSidebar>
    {#if device}
      {#key device}<DeviceProvider {device}><ChatShortcuts newChatHandle={handle} /></DeviceProvider>{/key}
    {/if}
  {/snippet}
  {#snippet topbar()}
    {#if device}
      {#key device}<DeviceProvider {device}><TopBar /></DeviceProvider>{/key}
    {:else}
      <header class={['flex items-center px-3', handle ? 'h-8' : 'h-12']}><Sidebar.Trigger class="md:hidden" /></header>
    {/if}
  {/snippet}
  {#snippet children()}
    {#if handle}
      {#if device}
        {#key device}<DeviceProvider {device}>{@render children()}</DeviceProvider>{/key}
      {:else}
        <RemoteDashboardPage
          {handle} state={dashboardState} onRetry={retry}
          unavailableMessage={$error ?? moduleError}
          {signInUrl}
        />
      {/if}
    {:else}
      <PublicNewChatPage devices={$devices} loading={$loading && !moduleError}
        error={$error ?? moduleError} onRetry={() => connections ? void connections.refreshDevices() : void start()}
        onRetryDevice={(handle) => connections?.retry(handle)} {signInUrl} />
    {/if}
  {/snippet}
</AppShell>
