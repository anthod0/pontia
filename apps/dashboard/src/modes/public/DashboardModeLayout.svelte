<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, type Snippet } from 'svelte';
  import { writable } from 'svelte/store';
  import DeviceSessionList from '../../components/layout/DeviceSessionList.svelte';
  import SidebarDeviceSwitcher from './SidebarDeviceSwitcher.svelte';
  import ComposerDeviceSelect from './ComposerDeviceSelect.svelte';
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon';
  import { Button } from '$lib/components/ui/button/index.js';
  import { dashboardRelativePath, dashboardScope, navigate } from '$lib/navigation';
  import { matchChatShortcut } from '$lib/shortcuts/definitions';
  import { adjacentValue } from '$lib/shortcuts/sessionNavigation';
  import DeviceStatus, { type DeviceStatusView } from '../../components/devices/DeviceStatus.svelte';
  import AppShell from '../../components/layout/AppShell.svelte';
  import AppSidebar from '../../components/layout/AppSidebar.svelte';
  import DeviceBotList from '../../components/bots/DeviceBotList.svelte';
  import TopBarMoreMenu from '../../components/layout/TopBarMoreMenu.svelte'
  import TopBar from '../../components/layout/TopBar.svelte';
  import ChatShortcuts from '../../components/chat/ChatShortcuts.svelte';
  import ConnectionNotice from '../../components/layout/ConnectionNotice.svelte';
  import NewChatPage from '../../pages/NewChatPage.svelte';
  import BotEntryPage from '../../pages/BotEntryPage.svelte';
  import NewChatLayout from '../../components/chat/NewChatLayout.svelte';
  import NewChatComposerPlaceholder from '../../components/chat/NewChatComposerPlaceholder.svelte';
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
  let currentLocation = $state(window.location.pathname);
  const currentPath = $derived(dashboardRelativePath(currentLocation));
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

  // Connection progress or failure for the device the page is waiting on.
  const deviceStatus = $derived.by((): DeviceStatusView | null => {
    if (dashboardState === 'connecting' && (!checkingDevices || activeHandle)) return {
      mood: 'loading',
      title: $loading ? $t('devices.loading') : $t('devices.connectingNamed', { name: active?.name ?? activeHandle }),
      description: $t('common.fewSeconds'),
    };
    if (dashboardState !== 'unavailable' || !activeHandle) return null;
    if (active?.state === 'offline') return {
      mood: 'offline',
      title: $t('devices.offlineNamed', { name: active.name }),
      description: $t('devices.checkRunning'),
    };
    return {
      mood: 'offline',
      title: $t('devices.unavailableNamed', { name: active?.name ?? activeHandle }),
      description: $error ?? moduleError ?? $t('devices.connectionFailed'),
    };
  });

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

  $effect(() => {
    const sessionId = currentPath.match(/^\/chat\/([^/]+)$/)?.[1];
    const target = $devices.find(target => `/${target.handle}` === dashboardScope(currentLocation));
    if (!sessionId || !target) return;
    try {
      window.sessionStorage.setItem(`pontia.lastSession:${target.deviceId}`, sessionId);
    } catch (error) {
      if (!(error instanceof DOMException)) throw error;
    }
  });

  function selectDevice(target: string): void {
    const path = dashboardRelativePath();
    const targetDevice = $devices.find(device => device.handle === target)!;
    let sessionId: string | null = null;
    try {
      sessionId = window.sessionStorage.getItem(`pontia.lastSession:${targetDevice.deviceId}`);
    } catch (error) {
      if (!(error instanceof DOMException)) throw error;
    }
    const destination = handle && path.startsWith('/settings/') ? path : sessionId ? `/chat/${sessionId}` : '/';
    void navigate(destination, undefined, { scope: `/${target}` });
  }

  function handleDeviceShortcut(event: KeyboardEvent): void {
    if ($devices.length < 2) return;
    const shortcut = matchChatShortcut(event);
    if (shortcut?.action !== 'switchDevice') return;
    const target = adjacentValue($devices.map(device => device.handle), activeHandle ?? null, shortcut.direction);
    if (!target) return;
    event.preventDefault();
    selectDevice(target);
  }

  function openNewChat(): void {
    void navigate('/', undefined, { scope: activeHandle ? `/${activeHandle}` : '' });
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
      if (!disposed) moduleError = $t('devices.remoteAccessFailed');
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

<svelte:window onkeydown={handleDeviceShortcut} onpopstate={() => currentLocation = window.location.pathname} onfocus={() => connections?.focus()} ononline={() => connections?.focus()} />
<svelte:document onvisibilitychange={() => {
  if (document.visibilityState === 'visible') connections?.focus();
}} />

{#snippet statusAction()}
  {#if deviceStatus?.mood === 'offline'}<Button variant="outline" onclick={retry}>{$t('common.tryAgain')}</Button>{/if}
{/snippet}

{#snippet deviceSelector(disabled: boolean)}
  {#if $devices.length > 1}
    <ComposerDeviceSelect devices={$devices} selected={active} onSelect={selectDevice} {disabled} />
  {/if}
{/snippet}

<AppShell>
  {#snippet sidebar()}
    <AppSidebar onNewChat={openNewChat}
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
            <DeviceProvider device={active.runtime}>
              <DeviceBotList handle={active.handle} />
              <DeviceSessionList handle={active.handle} />
            </DeviceProvider>
          {/key}
        {:else if active?.state === 'offline'}
          <div class="flex items-center justify-between gap-2 px-4 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
            <span>{active.name} {$t('devices.offlineSuffix')}</span>
            <Button variant="outline" size="xs" onclick={() => connections?.retry(active.handle)}>{$t('common.retry')}</Button>
          </div>
        {:else if dashboardState === 'connecting'}
          <Sidebar.Group class="group-data-[collapsible=icon]:hidden">
            {#if !$loading}<Sidebar.GroupLabel role="status">{$t('connection.connecting')}</Sidebar.GroupLabel>{/if}
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
      {#key device}<DeviceProvider {device}><ChatShortcuts newChatHandle={activeHandle} /><ConnectionNotice /></DeviceProvider>{/key}
    {/if}
  {/snippet}
  {#snippet topbar()}
    {#if device}
      {#key device}<DeviceProvider {device}><TopBar onNewChat={openNewChat} /></DeviceProvider>{/key}
    {:else}
      <header class="flex h-12 items-center px-3"><Sidebar.Trigger class="size-11 md:hidden [&_svg:not([class*=size-])]:size-5" /><TopBarMoreMenu /></header>
    {/if}
  {/snippet}
  {#snippet children()}
    {#if device}
      {#key device}
        <DeviceProvider {device}>
          {#if handle}
            {@render children()}
          {:else if currentPath === '/bot'}
            <BotEntryPage routeScope={`/${activeHandle}`} />
          {:else}
            <NewChatPage routeScope={`/${activeHandle}`} {deviceSelector} />
          {/if}
        </DeviceProvider>
      {/key}
    {:else if signInUrl}
      <div class="flex min-h-96 flex-col items-center justify-center gap-3 bg-surface p-4" role="status">
        <SpinnerGapIcon class="size-6 animate-spin text-primary" aria-hidden="true" />
        <p class="text-sm text-muted-foreground">{$t('auth.redirecting')}</p>
        <Button size="sm" variant="outline" href={signInUrl}>{$t('auth.signIn')}</Button>
      </div>
    {:else if deviceStatus}
      {#if currentPath === '/'}
        <NewChatLayout status={deviceStatus}>
          {#snippet action()}{@render statusAction()}{/snippet}
          <NewChatComposerPlaceholder {deviceSelector} />
        </NewChatLayout>
      {:else}
        <div class="flex min-h-[calc(100svh-5rem)] items-center justify-center py-12">
          <DeviceStatus status={deviceStatus}>
            {#snippet action()}{@render statusAction()}{/snippet}
          </DeviceStatus>
        </div>
      {/if}
    {:else if $error ?? moduleError}
      <div role="alert" class="flex items-center gap-3 py-4 text-sm">
        <span>{$error ?? moduleError}</span>
        <Button variant="outline" onclick={retry}>{$t('common.tryAgain')}</Button>
      </div>
    {:else}
      <DeviceOnboarding checking={checkingDevices} onCheck={checkDevices} />
    {/if}
  {/snippet}
</AppShell>
