<script lang="ts">
  import type { Snippet } from 'svelte';
  import DevicesPage from '../../components/devices/DevicesPage.svelte';
  import RemoteDashboardPage from '../../components/devices/RemoteDashboardPage.svelte';
  import { initialRemoteDashboardState, isValidDeviceHandle, type RemoteDashboardState } from '$lib/remoteDashboard';
  import { clearDashboardRuntimeState, startDashboardRuntime, stopDashboardRuntime } from '../../services/dashboardRuntime';
  import { clearPublicApiTarget, setPublicApiTarget } from './apiTarget';
  import { clearE2eSession } from './e2eTransport';
  import {
    dashboardBootstrapUrl,
    dashboardSignInUrl,
    listPublicDevices,
    resolvePublicDeviceTarget,
    CloudRequestError,
    type PublicDevice,
  } from './remoteAccess';

  let { handle, children }: { handle?: string; children: Snippet } = $props();
  let retryGeneration = $state(0);
  let runtimeHandle = $state<string | null>(null);
  let failedHandle = $state<string | null>(null);
  let targetFailureMessage = $state<string | null>(null);
  let targetSignInRequired = $state(false);
  let devices = $state<PublicDevice[]>([]);
  let devicesLoading = $state(false);
  let devicesError = $state<string | null>(null);
  let devicesSignInRequired = $state(false);
  const signInUrl = dashboardSignInUrl(
    `${window.location.pathname}${window.location.search}${window.location.hash}`,
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
    retryGeneration += 1;
  }

  function teardownRuntime(): void {
    clearPublicApiTarget();
    clearE2eSession();
    stopDashboardRuntime();
    clearDashboardRuntimeState();
    runtimeHandle = null;
  }

  function cloudFailureMessage(error: unknown, operation: 'list' | 'target'): string {
    if (error instanceof CloudRequestError && error.status === 401) {
      return 'Sign in to Pontia to continue.';
    }
    if (error instanceof CloudRequestError && error.status === 403) {
      return 'Pontia refused this Dashboard request.';
    }
    if (error instanceof CloudRequestError) {
      return operation === 'list'
        ? 'Pontia returned an invalid device list.'
        : 'Pontia could not confirm this device target.';
    }
    return operation === 'list'
      ? 'Could not reach Pontia to load devices.'
      : 'Could not reach Pontia to confirm this device target.';
  }

  $effect.pre(() => {
    if (runtimeHandle && runtimeHandle !== handle) teardownRuntime();
  });

  $effect(() => {
    retryGeneration;
    if (handle) return;
    const controller = new AbortController();
    devicesLoading = true;
    devicesError = null;
    devicesSignInRequired = false;
    void listPublicDevices(controller.signal)
      .then((loaded) => {
        if (!controller.signal.aborted) devices = loaded;
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted) {
          devicesError = cloudFailureMessage(error, 'list');
          devicesSignInRequired = error instanceof CloudRequestError && error.status === 401;
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) devicesLoading = false;
      });
    return () => controller.abort();
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

    if (import.meta.env.DEV && import.meta.env.MODE === 'development') {
      runtimeHandle = requestedHandle;
      startDashboardRuntime();
      return teardownRuntime;
    }

    const controller = new AbortController();
    void resolvePublicDeviceTarget(requestedHandle, controller.signal)
      .then((target) => {
        if (controller.signal.aborted || handle !== requestedHandle) return;
        setPublicApiTarget(target);
        runtimeHandle = requestedHandle;
        startDashboardRuntime();
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted || handle !== requestedHandle) return;
        failedHandle = requestedHandle;
        targetFailureMessage = cloudFailureMessage(error, 'target');
        targetSignInRequired = error instanceof CloudRequestError && error.status === 401;
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
    reauthorizationUrl={isValidDeviceHandle(handle) ? dashboardBootstrapUrl(handle) : undefined}
    signInUrl={targetSignInRequired ? signInUrl : undefined}
  >
    {#snippet dashboard()}
      {@render children()}
    {/snippet}
  </RemoteDashboardPage>
{:else}
  <DevicesPage
    {devices}
    loading={devicesLoading}
    error={devicesError}
    onRetry={retry}
    openAction={dashboardBootstrapUrl}
    signInUrl={devicesSignInRequired ? signInUrl : undefined}
  />
{/if}
