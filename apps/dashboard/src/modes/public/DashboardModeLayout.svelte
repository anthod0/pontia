<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { writable } from 'svelte/store';
  import DeviceSessionList from '../../components/layout/DeviceSessionList.svelte';
  import SidebarDeviceSwitcher from './SidebarDeviceSwitcher.svelte';
  import ComposerDeviceSelect from './ComposerDeviceSelect.svelte';
  import { Button } from '$lib/components/ui/button/index.js';
  import * as Empty from '$lib/components/ui/empty/index.js';
  import { dashboardRelativePath, navigate } from '$lib/navigation';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import AppSidebar from '../../components/layout/AppSidebar.svelte';
  import TopBar from '../../components/layout/TopBar.svelte';
  import ChatShortcuts from '../../components/chat/ChatShortcuts.svelte';
  import NewChatPage from '../../pages/NewChatPage.svelte';
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
  const remote = $derived(loadedRemoteAccess());
  let moduleError = $state<string | null>(null);
  let disposed = false;
  let loadingModule = false;
  const lastDeviceStorageKey = 'pontia.lastDeviceHandle';
  let lastHandle = $state<string | null>(null);
  // The routed device is active; the root uses the last routed device, then the first listed one.
  const activeHandle = $derived(handle ||
    ($devices.some(target => target.handle === lastHandle) ? lastHandle! : $devices[0]?.handle));
  const active = $derived($devices.find(target => target.handle === activeHandle));
  const device = $derived(active?.state === 'online' ? active.runtime : null);
  const signInUrl = $derived($signInRequired ? remote?.dashboardSignInUrl(
    `${window.location.pathname}${window.location.search}${window.location.hash}`,
  ) : undefined);
  const dashboardState = $derived(active?.state === 'online' ? 'available'
    : $signInRequired || active?.state === 'offline' || moduleError || (!$loading && !active)
      ? 'unavailable' : 'connecting');

  $effect(() => {
    if (handle && $devices.some(target => target.handle === handle)) rememberDevice(handle);
  });

  function rememberDevice(target: string): void {
    lastHandle = target;
    try { window.localStorage.setItem(lastDeviceStorageKey, target); } catch { /* Storage is optional. */ }
  }

  function selectDevice(target: string): void {
    const path = dashboardRelativePath();
    void navigate(handle && path.startsWith('/settings/') ? path : '/', undefined, { scope: `/${target}` });
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
    else if (active) connections.retry(active.handle);
    else void connections.refreshDevices();
  }

  onMount(() => {
    try { lastHandle = window.localStorage.getItem(lastDeviceStorageKey); } catch { /* Storage is optional. */ }
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

{#snippet deviceSelector(disabled: boolean)}
  <ComposerDeviceSelect devices={$devices} selected={active} onSelect={selectDevice} {disabled} />
{/snippet}

<AppShell>
  {#snippet sidebar()}
    <AppSidebar onNewChat={() => void navigate('/', undefined, { scope: '' })}
      settingsDisabled={!activeHandle}
      onSettings={() => { if (activeHandle) void navigate('/settings/common', undefined, { scope: `/${activeHandle}` }) }}>
      {#snippet groups()}
        {#if $devices.length}
          <SidebarDeviceSwitcher devices={$devices} selected={active} onSelect={selectDevice} />
        {/if}
        {#if active?.state === 'online' && active.runtime}
          {#key active.runtime}
            <DeviceProvider device={active.runtime}><DeviceSessionList handle={active.handle} /></DeviceProvider>
          {/key}
        {:else if active?.state === 'offline'}
          <div class="flex items-center justify-between gap-2 px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
            <span>{active.name} is offline.</span>
            <Button variant="outline" size="xs" onclick={() => connections?.retry(active.handle)}>Retry</Button>
          </div>
        {:else if active?.state === 'connecting'}
          <p role="status" class="px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">Connecting…</p>
        {/if}
      {/snippet}
    </AppSidebar>
    {#if device}
      {#key device}<DeviceProvider {device}><ChatShortcuts newChatHandle={activeHandle} /></DeviceProvider>{/key}
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
    {#if device}
      {#key device}
        <DeviceProvider {device}>
          {#if handle}{@render children()}{:else}<NewChatPage routeScope={`/${activeHandle}`} {deviceSelector} />{/if}
        </DeviceProvider>
      {/key}
    {:else if activeHandle}
      <RemoteDashboardPage
        handle={activeHandle} state={dashboardState} onRetry={retry}
        unavailableMessage={$error ?? moduleError}
        {signInUrl}
      />
    {:else if $error ?? moduleError}
      <div role="alert" class="flex items-center gap-3 py-4 text-sm">
        <span>{$error ?? moduleError}</span>
        {#if signInUrl}<Button href={signInUrl}>Sign in</Button>
        {:else}<Button variant="outline" onclick={retry}>Try again</Button>{/if}
      </div>
    {:else if $loading}
      <p role="status" class="py-4 text-sm text-muted-foreground">Loading devices…</p>
    {:else}
      <Empty.Root class="min-h-[60svh]">
        <Empty.Header>
          <Empty.Title>No devices available</Empty.Title>
          <Empty.Description>Connect a device to Pontia to start a session.</Empty.Description>
        </Empty.Header>
        <Empty.Content><Button href="https://pontia.dev">Manage devices</Button></Empty.Content>
      </Empty.Root>
    {/if}
  {/snippet}
</AppShell>
