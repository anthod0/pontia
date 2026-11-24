<script lang="ts">
  import { t } from '$lib/i18n';
  import { onMount, type Snippet } from 'svelte';
  import { writable } from 'svelte/store';
  import { dashboardRelativePath, navigate } from '$lib/navigation';
  import { returnToLastPage } from '$lib/lastPage';
  import { PersistedState, textSerializer } from '$lib/storage';
  import { matchChatShortcut } from '$lib/shortcuts/definitions';
  import type { DeviceStatusView } from "../../components/devices/DeviceStatus.svelte";
  import DashboardContent from "./DashboardContent.svelte";
  import AppShell from '../../shell/AppShell.svelte';
  import { createDeviceConnections, type ConnectedDevice } from './deviceConnections';
  import { provideDashboardHost, type HostDevice } from '../../device/hostContext';
  import { loadedRemoteAccess, loadRemoteAccess } from './remoteAccessLoader.svelte';
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
  const lastHandle = new PersistedState('pontia.lastDeviceHandle', null, textSerializer);
  const previousHandle = new PersistedState('pontia.previousDeviceHandle', null, textSerializer);
  // The device shown before the active one changed; starts from the last device of the previous visit.
  let shownHandle = lastHandle.current;
  const hostDevices = $derived($devices.map((target): HostDevice => ({
    id: target.deviceId, handle: target.handle, name: target.name, runtime: target.runtime,
    state: target.state === 'sign-in-required' ? 'sign_in_required' : target.state,
  })));
  // The routed device is active; the root uses the last routed device, then the first listed one.
  const activeHandle = $derived(handle ||
    (hostDevices.some(target => target.handle === lastHandle.current) ? lastHandle.current! : hostDevices[0]?.handle));
  const active = $derived(hostDevices.find(target => target.handle === activeHandle));
  // The device shown before the active one, while it is still listed.
  const previous = $derived(previousHandle.current === activeHandle ? undefined
    : hostDevices.find(target => target.handle === previousHandle.current));
  const loadError = $derived($error ?? moduleError);
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
      description: loadError ?? $t('devices.connectionFailed'),
    };
  });

  provideDashboardHost({
    mode: 'public',
    get devices() { return hostDevices; },
    get activeDevice() { return active ?? null; },
    get previousDevice() { return previous ?? null; },
    get canSwitchDevice() { return hostDevices.length > 1; },
    get routeScope() { return activeHandle ? `/${activeHandle}` : undefined; },
    get statusView() { return deviceStatus; },
    get signInUrl() { return signInUrl; },
    get loadError() { return loadError; },
    get onboarding() { return dashboardState !== 'available' && !signInUrl && !deviceStatus && !loadError; },
    get checkingDevices() { return checkingDevices; },
    get loadingDevices() { return $loading; },
    get connecting() { return dashboardState === 'connecting'; },
    selectDevice, retryActiveOrRefresh: retry, retryDevice: target => connections?.retry(target),
    checkDevices, openNewChat,
    openSettings() { if (activeHandle) void navigate('/settings/common', undefined, { scope: `/${activeHandle}` }); },
  });

  // A signed-out browser goes straight to Pontia sign-in, which returns to this page.
  $effect(() => {
    if (signInUrl) redirectToSignIn(signInUrl);
  });

  $effect(() => {
    if (handle && hostDevices.some(target => target.handle === handle)) rememberDevice(handle);
  });

  $effect(() => {
    const current = active?.handle;
    if (!current || current === shownHandle) return;
    if (hostDevices.some(target => target.handle === shownHandle)) rememberPreviousDevice(shownHandle!);
    shownHandle = current;
  });

  function rememberDevice(target: string): void {
    lastHandle.current = target;
  }

  function rememberPreviousDevice(target: string): void {
    previousHandle.current = target;
  }

  // Settings keep their section; elsewhere the target device reopens its last chat.
  function selectDevice(target: string): void {
    const path = dashboardRelativePath();
    if (handle && path.startsWith('/settings/')) {
      void navigate(path, undefined, { scope: `/${target}` });
      return;
    }
    returnToLastPage();
    void navigate('/', undefined, { scope: `/${target}` });
  }

  function handleDeviceShortcut(event: KeyboardEvent): void {
    if (!previous || matchChatShortcut(event)?.action !== 'previousDevice') return;
    event.preventDefault();
    // Holding the key would bounce between the two devices.
    if (event.repeat) return;
    selectDevice(previous.handle);
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
    void start();
    return () => {
      disposed = true;
      connections?.dispose();
    };
  });
</script>

<svelte:window onkeydown={handleDeviceShortcut} onfocus={() => connections?.focus()} ononline={() => connections?.focus()} />
<svelte:document onvisibilitychange={() => {
  if (document.visibilityState === 'visible') connections?.focus();
}} />

<AppShell>
  <DashboardContent {handle}>{@render children()}</DashboardContent>
</AppShell>
