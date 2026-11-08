<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { writable } from 'svelte/store';
  import DevicesPage from '../../components/devices/DevicesPage.svelte';
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

  async function start(): Promise<void> {
    if (loadingModule) return;
    loadingModule = true;
    moduleError = null;
    try {
      const access = await loadRemoteAccess();
      if (disposed) return;
      const developmentBridge = import.meta.env.DEV && import.meta.env.MODE === 'development';
      const developmentHandle = handle;
      connections = createDeviceConnections(developmentBridge && developmentHandle ? {
        ...access, listPublicDevices: async () => [{ handle: developmentHandle, name: developmentHandle }],
      } : access);
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

<AppShell settingsEnabled={!!device}>
  {#snippet sidebar()}
    {#if device}
      {#key device}
        <DeviceProvider {device}>
          <ChatShortcuts />
          <AppSidebar />
        </DeviceProvider>
      {/key}
    {:else}
      <Sidebar.Root collapsible="icon">
        <Sidebar.Header><a href="/" class="px-2 py-1 font-semibold">Pontia</a></Sidebar.Header>
        <Sidebar.Content>
          <Sidebar.Group>
            <Sidebar.Menu>
              <Sidebar.MenuItem><Sidebar.MenuButton>
                {#snippet child({ props })}<a {...props} href="/">Devices</a>{/snippet}
              </Sidebar.MenuButton></Sidebar.MenuItem>
              {#each $devices as target (target.handle)}
                <Sidebar.MenuItem>
                  <Sidebar.MenuButton isActive={handle === target.handle}>
                    {#snippet child({ props })}
                      <a {...props} href={`/${target.handle}`}>
                        <span>{target.name}</span>
                        <span class="ml-auto text-xs">{{ connecting: 'Connecting', online: 'Online', offline: 'Offline', 'sign-in-required': 'Sign in required' }[target.state]}</span>
                      </a>
                    {/snippet}
                  </Sidebar.MenuButton>
                </Sidebar.MenuItem>
              {/each}
            </Sidebar.Menu>
          </Sidebar.Group>
        </Sidebar.Content>
      </Sidebar.Root>
    {/if}
  {/snippet}
  {#snippet topbar()}
    {#if device}
      {#key device}<DeviceProvider {device}><TopBar /></DeviceProvider>{/key}
    {:else}
      <header class="flex h-8 items-center px-3"><Sidebar.Trigger class="md:hidden" /></header>
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
      <DevicesPage devices={$devices} loading={$loading && !moduleError}
        error={$error ?? moduleError} onRetry={() => connections ? void connections.refreshDevices() : void start()}
        openAction={(target) => `/${target}`} {signInUrl} />
    {/if}
  {/snippet}
</AppShell>
