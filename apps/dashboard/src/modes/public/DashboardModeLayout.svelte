<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { writable } from 'svelte/store';
  import DeviceSessionList from '../../components/layout/DeviceSessionList.svelte';
  import SidebarDeviceSwitcher from './SidebarDeviceSwitcher.svelte';
  import ComposerDeviceSelect from './ComposerDeviceSelect.svelte';
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon';
  import { Button } from '$lib/components/ui/button/index.js';
  import { dashboardRelativePath, navigate } from '$lib/navigation';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import AppSidebar from '../../components/layout/AppSidebar.svelte';
  import TopBar from '../../components/layout/TopBar.svelte';
  import ChatShortcuts from '../../components/chat/ChatShortcuts.svelte';
  import NewChatPage from '../../pages/NewChatPage.svelte';
  import NewChatLayout from '../../components/chat/NewChatLayout.svelte';
  import NewChatComposerPlaceholder from '../../components/chat/NewChatComposerPlaceholder.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton/index.js';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import DeviceProvider from '../../device/DeviceProvider.svelte';
  import { createDeviceConnections, type ConnectedDevice } from './deviceConnections';
  import { provideDeviceList } from './deviceListContext';
  import { loadedRemoteAccess, loadRemoteAccess } from './remoteAccessLoader.svelte';
  import DeviceOnboarding from './DeviceOnboarding.svelte';
  import { redirectToSignIn } from './signInRedirect';

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
  let currentPath = $state(dashboardRelativePath());
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

  provideDeviceList({
    devices: () => $devices,
    retry: (target) => connections?.retry(target),
  });

  // A signed-out browser goes straight to Pontia sign-in, which returns to this page.
  $effect(() => {
    if (signInUrl) redirectToSignIn(signInUrl);
  });

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

  let checkingDevices = $state(false);

  async function checkDevices(): Promise<void> {
    checkingDevices = true;
    try { await connections?.refreshDevices(); } finally { checkingDevices = false; }
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

<svelte:window onpopstate={() => currentPath = dashboardRelativePath()} onfocus={() => connections?.focus()} ononline={() => connections?.focus()} />
<svelte:document onvisibilitychange={() => {
  if (document.visibilityState === 'visible') connections?.focus();
}} />

{#snippet deviceSelector(disabled: boolean)}
  {#if $devices.length > 1}
    <ComposerDeviceSelect devices={$devices} selected={active} onSelect={selectDevice} {disabled} />
  {/if}
{/snippet}

<AppShell>
  {#snippet sidebar()}
    <AppSidebar onNewChat={() => void navigate('/', undefined, { scope: activeHandle ? `/${activeHandle}` : '' })}
      settingsDisabled={!activeHandle}
      onSettings={() => { if (activeHandle) void navigate('/settings/common', undefined, { scope: `/${activeHandle}` }) }}>
      {#snippet deviceSwitcher()}
        {#if $devices.length > 1}
          <SidebarDeviceSwitcher devices={$devices} selected={active} onSelect={selectDevice} />
        {/if}
      {/snippet}
      {#snippet groups()}
        {#if active?.state === 'online' && active.runtime}
          {#key active.runtime}
            <DeviceProvider device={active.runtime}><DeviceSessionList handle={active.handle} /></DeviceProvider>
          {/key}
        {:else if active?.state === 'offline'}
          <div class="flex items-center justify-between gap-2 px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
            <span>{active.name} is offline.</span>
            <Button variant="outline" size="xs" onclick={() => connections?.retry(active.handle)}>Retry</Button>
          </div>
        {:else if dashboardState === 'connecting'}
          <Sidebar.Group class="group-data-[collapsible=icon]:hidden">
            {#if !$loading}<Sidebar.GroupLabel role="status">Connecting…</Sidebar.GroupLabel>{/if}
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
      {#key device}<DeviceProvider {device}><ChatShortcuts newChatHandle={activeHandle} /></DeviceProvider>{/key}
    {/if}
  {/snippet}
  {#snippet topbar()}
    {#if device}
      {#key device}<DeviceProvider {device}><TopBar /></DeviceProvider>{/key}
    {:else}
      <header class={['flex items-center px-3', currentPath === '/' || currentPath.startsWith('/chat/') ? 'h-12' : 'h-8']}><Sidebar.Trigger class="md:hidden" /></header>
    {/if}
  {/snippet}
  {#snippet children()}
    {#if device}
      {#key device}
        <DeviceProvider {device}>
          {#if handle}{@render children()}{:else}<NewChatPage routeScope={`/${activeHandle}`} {deviceSelector} />{/if}
        </DeviceProvider>
      {/key}
    {:else if signInUrl}
      <div class="flex min-h-96 flex-col items-center justify-center gap-3 bg-surface p-4" role="status">
        <SpinnerGapIcon class="size-6 animate-spin text-primary" aria-hidden="true" />
        <p class="text-sm text-muted-foreground">Redirecting to sign in…</p>
        <Button size="sm" variant="outline" href={signInUrl}>Sign in</Button>
      </div>
    {:else if dashboardState === 'connecting' && (!checkingDevices || activeHandle)}
      {#if currentPath === '/'}
        <NewChatLayout status={{
          mood: 'loading',
          title: $loading ? 'Loading devices…' : `Connecting to ${active?.name ?? activeHandle}…`,
          description: 'This usually takes a few seconds.',
        }}>
          <NewChatComposerPlaceholder {deviceSelector} />
        </NewChatLayout>
      {:else}
        <div class="space-y-4 py-4" role="status" aria-label="Loading device content">
          <Skeleton class="h-6 w-48" />
          <Skeleton class="h-24 w-full" />
          <Skeleton class="h-40 w-full" />
        </div>
      {/if}
    {:else if active?.state === 'offline' && currentPath === '/'}
      <NewChatLayout status={{
        mood: 'offline',
        title: `${active.name} is offline`,
        description: 'Make sure Pontia is running on this device, then try again.',
      }}>
        {#snippet action()}<Button variant="outline" onclick={retry}>Try again</Button>{/snippet}
        <NewChatComposerPlaceholder {deviceSelector} />
      </NewChatLayout>
    {:else if activeHandle}
      <RemoteDashboardPage
        handle={activeHandle} state={dashboardState} onRetry={retry}
        unavailableMessage={$error ?? moduleError}
      />
    {:else if $error ?? moduleError}
      <div role="alert" class="flex items-center gap-3 py-4 text-sm">
        <span>{$error ?? moduleError}</span>
        <Button variant="outline" onclick={retry}>Try again</Button>
      </div>
    {:else}
      <DeviceOnboarding checking={checkingDevices} onCheck={checkDevices} />
    {/if}
  {/snippet}
</AppShell>
