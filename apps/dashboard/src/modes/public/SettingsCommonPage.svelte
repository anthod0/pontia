<script lang="ts">
  import { onMount } from 'svelte';
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon';
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon';
  import UserIcon from 'phosphor-svelte/lib/UserIcon';
  import * as Avatar from '$lib/components/ui/avatar/index.js';
  import { Button } from '$lib/components/ui/button/index.js';
  import { Skeleton } from '$lib/components/ui/skeleton/index.js';
  import { deviceStateLabel } from './deviceConnections';
  import { useDeviceList } from './deviceListContext';
  import { loadRemoteAccess, type RemoteAccess } from './remoteAccessLoader.svelte';

  const deviceList = useDeviceList();
  const devices = $derived(deviceList.devices());
  let profile = $state<Awaited<ReturnType<RemoteAccess['getCloudProfile']>> | null>(null);
  let accountUrl = $state<string | null>(null);
  let devicesUrl = $state<string | null>(null);
  let failed = $state(false);

  onMount(() => {
    const request = new AbortController();
    loadRemoteAccess()
      .then(async (remote) => {
        accountUrl = remote.cloudAccountUrl();
        devicesUrl = remote.cloudDevicesUrl();
        profile = await remote.getCloudProfile(request.signal);
      })
      .catch(() => {
        if (!request.signal.aborted) failed = true;
      });
    return () => request.abort();
  });
</script>

<div class="space-y-10">
  <section class="space-y-4" aria-labelledby="settings-account-title">
    <div class="flex items-center justify-between gap-4">
      <h2 id="settings-account-title" class="text-sm font-semibold text-heading">Account</h2>
      {#if accountUrl}
        <Button variant="outline" size="sm" href={accountUrl} target="_blank" rel="noopener">
          Manage account <ArrowSquareOutIcon class="size-4" />
        </Button>
      {/if}
    </div>
    <div class="flex min-w-0 items-center gap-3">
      {#if profile}
        <Avatar.Root size="lg">
          {#if profile.avatarUrl}<Avatar.Image src={profile.avatarUrl} alt="" referrerpolicy="no-referrer" />{/if}
          <Avatar.Fallback><UserIcon class="size-5" /></Avatar.Fallback>
        </Avatar.Root>
        <div class="min-w-0">
          <p class="truncate font-medium text-heading">{profile.displayName ?? 'Pontia user'}</p>
          <p class="text-xs text-muted-foreground">Signed in to Pontia Cloud</p>
        </div>
      {:else if failed}
        <p class="text-sm text-muted-foreground">Could not load your Pontia Cloud account.</p>
      {:else}
        <Skeleton class="size-10 rounded-full" />
        <div class="space-y-1.5" role="status" aria-label="Loading account">
          <Skeleton class="h-4 w-32" />
          <Skeleton class="h-3 w-24" />
        </div>
      {/if}
    </div>
  </section>

  <section class="space-y-4" aria-labelledby="settings-devices-title">
    <div class="flex items-center justify-between gap-4">
      <h2 id="settings-devices-title" class="text-sm font-semibold text-heading">Devices</h2>
      {#if devicesUrl}
        <Button variant="outline" size="sm" href={devicesUrl} target="_blank" rel="noopener">
          Manage devices <ArrowSquareOutIcon class="size-4" />
        </Button>
      {/if}
    </div>
    <ul class="-mx-3" aria-label="Devices">
      {#each devices as device (device.handle)}
        <li class="flex items-center gap-3 px-3 py-2.5">
          <DesktopIcon class="size-5 shrink-0 text-muted-foreground" aria-hidden="true" />
          <div class="min-w-0 flex-1">
            <p class="truncate font-medium text-heading">{device.name}</p>
            <p class="truncate font-mono text-xs text-muted-foreground">{device.handle}</p>
          </div>
          <span class="shrink-0 text-xs text-muted-foreground">{deviceStateLabel[device.state]}</span>
          {#if device.state === 'offline'}
            <Button variant="outline" size="sm" aria-label={`Retry ${device.name}`} onclick={() => deviceList.retry(device.handle)}>Retry</Button>
          {/if}
        </li>
      {:else}
        <li class="px-3 py-2.5 text-sm text-muted-foreground">No devices registered.</li>
      {/each}
    </ul>
  </section>
</div>
