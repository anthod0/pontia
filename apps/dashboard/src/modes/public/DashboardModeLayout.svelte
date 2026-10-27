<script lang="ts">
  import { untrack, type Snippet } from 'svelte';
  import DevicesPage from '../../components/devices/DevicesPage.svelte';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import { initialRemoteDashboardState, type RemoteDashboardState } from '$lib/remoteDashboard';
  import { isValidDeviceHandle } from '$lib/deviceHandle';
  import { clearDashboardRuntimeState, startDashboardRuntime, stopDashboardRuntime } from '../../services/dashboardRuntime';
  import { createPublicDevicesQuery } from './deviceQueries';
  import { loadedRemoteAccess, loadRemoteAccess } from './remoteAccessLoader.svelte';

  let { handle: handleProp, children }: { handle?: string; children: Snippet } = $props();
  // Route data may be replaced during navigation without changing the device.
  const handle = $derived(handleProp);
  let retryGeneration = $state(0);
  let runtimeHandle = $state<string | null>(null);
  let failedHandle = $state<string | null>(null);
  let targetFailureMessage = $state<string | null>(null);
  let targetSignInRequired = $state(false);
  const remote = $derived(loadedRemoteAccess());
  const devicesQuery = createPublicDevicesQuery(() => !handle);
  const devices = $derived(devicesQuery.data ?? []);
  const devicesError = $derived(
    devicesQuery.error ? cloudFailureMessage(devicesQuery.error, 'list') : null,
  );
  const devicesSignInRequired = $derived(cloudErrorStatus(devicesQuery.error) === 401);
  const signInUrl = $derived(
    remote?.dashboardSignInUrl(
      `${window.location.pathname}${window.location.search}${window.location.hash}`,
    ),
  );

  const dashboardState = $derived.by((): RemoteDashboardState => {
    if (!handle) return 'invalid';
    const initial = initialRemoteDashboardState(handle);
    if (initial === 'invalid') return initial;
    if (runtimeHandle === handle) return 'available';
    if (failedHandle === handle) return 'unavailable';
    return 'connecting';
  });

  function retry(): void {
    if (!handle) {
      void devicesQuery.refetch();
      return;
    }
    retryGeneration += 1;
  }

  function teardownRuntime(): void {
    // Not a dependency of the connection effects: loading remote access must not reconnect.
    const remoteAccess = untrack(() => remote);
    remoteAccess?.clearPublicApiTarget();
    remoteAccess?.clearE2eSession();
    stopDashboardRuntime();
    clearDashboardRuntimeState();
    runtimeHandle = null;
  }

  // Returns null when the error is not a Cloud request error.
  function cloudErrorStatus(error: unknown): number | undefined | null {
    return remote && error instanceof remote.CloudRequestError ? error.status : null;
  }

  function cloudFailureMessage(error: unknown, operation: 'list' | 'target'): string {
    const status = cloudErrorStatus(error);
    if (status === 401) {
      return 'Sign in to Pontia to continue.';
    }
    if (status === 403) {
      return 'Pontia refused this Dashboard request.';
    }
    if (status !== null) {
      return operation === 'list'
        ? 'Pontia returned an invalid device list.'
        : 'Pontia could not authorize this device connection.';
    }
    return operation === 'list'
      ? 'Could not reach Pontia to load devices.'
      : 'Could not establish a secure connection to this device.';
  }

  $effect.pre(() => {
    if (runtimeHandle && runtimeHandle !== handle) teardownRuntime();
  });

  $effect(() => {
    retryGeneration;
    const requestedHandle = handle;
    if (!requestedHandle) return;

    teardownRuntime();
    failedHandle = null;
    targetFailureMessage = null;
    targetSignInRequired = false;
    if (!isValidDeviceHandle(requestedHandle)) return;

    const developmentBridge = import.meta.env.DEV && import.meta.env.MODE === 'development';
    const controller = new AbortController();
    void loadRemoteAccess()
      .then(async (remoteAccess) => {
        if (developmentBridge) return;
        const target = await remoteAccess.connectPublicDevice(requestedHandle, controller.signal);
        if (controller.signal.aborted || handle !== requestedHandle) return;
        remoteAccess.setPublicApiTarget(target);
      })
      .then(() => {
        if (controller.signal.aborted || handle !== requestedHandle) return;
        runtimeHandle = requestedHandle;
        startDashboardRuntime();
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted || handle !== requestedHandle) return;
        failedHandle = requestedHandle;
        targetFailureMessage = cloudFailureMessage(error, 'target');
        targetSignInRequired = cloudErrorStatus(error) === 401;
      });

    return () => {
      controller.abort();
      teardownRuntime();
    };
  });
</script>

{#if handle}
  <RemoteDashboardPage
    {handle}
    state={dashboardState}
    onRetry={retry}
    unavailableMessage={targetFailureMessage}
    reauthorizationUrl={isValidDeviceHandle(handle) ? remote?.dashboardBootstrapUrl(handle) : undefined}
    signInUrl={targetSignInRequired ? signInUrl : undefined}
  >
    {#snippet dashboard()}
      {@render children()}
    {/snippet}
  </RemoteDashboardPage>
{:else}
  <DevicesPage
    {devices}
    loading={devicesQuery.isPending}
    error={devicesError}
    onRetry={retry}
    openAction={remote?.dashboardBootstrapUrl}
    signInUrl={devicesSignInRequired ? signInUrl : undefined}
  />
{/if}
