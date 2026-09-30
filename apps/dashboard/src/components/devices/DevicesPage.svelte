<script lang="ts">
  import DesktopTowerIcon from 'phosphor-svelte/lib/DesktopTowerIcon';
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon';
  import { Button } from '$lib/components/ui/button/index.js';
  import * as Card from '$lib/components/ui/card/index.js';
  import * as Empty from '$lib/components/ui/empty/index.js';
  import { isValidDeviceHandle } from '$lib/remoteDashboard';

  export type DeviceListItem = {
    handle: string;
    name: string;
  };

  let { devices = [] }: { devices?: DeviceListItem[] } = $props();
  const validDevices = $derived(devices.filter((device) => isValidDeviceHandle(device.handle)));
</script>

<main class="min-h-svh bg-surface">
  <div class="mx-auto w-full max-w-5xl px-4 py-8 sm:px-8 sm:py-12">
    <header class="mb-8 flex flex-col gap-4 border-b border-border pb-6 sm:flex-row sm:items-end sm:justify-between">
      <div class="space-y-2">
        <p class="text-xs font-semibold tracking-[0.2em] text-primary">PONTIA</p>
        <div>
          <h1 class="text-2xl font-semibold text-heading">Devices</h1>
          <p class="mt-1 text-sm text-muted-foreground">Choose a device to open its Dashboard.</p>
        </div>
      </div>
      <Button href="https://pontia.dev" variant="outline">
        <PlusIcon /> Manage devices
      </Button>
    </header>

    {#if validDevices.length}
      <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {#each validDevices as device (device.handle)}
          <Card.Root>
            <Card.Header>
              <span class="flex size-9 shrink-0 items-center justify-center bg-muted text-primary">
                <DesktopTowerIcon class="size-5" />
              </span>
              <div class="mt-2 min-w-0">
                <Card.Title>{device.name}</Card.Title>
                <Card.Description class="truncate font-mono">{device.handle}</Card.Description>
              </div>
            </Card.Header>
            <Card.Footer class="border-t border-border bg-muted/30 px-4 py-3">
              <Button href={`/${device.handle}`} class="ml-auto">Open Dashboard</Button>
            </Card.Footer>
          </Card.Root>
        {/each}
      </div>
    {:else}
      <Card.Root>
        <Card.Content>
          <Empty.Root class="min-h-72 border border-dashed border-border">
            <Empty.Header>
              <Empty.Media variant="icon">
                <DesktopTowerIcon />
              </Empty.Media>
              <Empty.Title>No devices yet</Empty.Title>
              <Empty.Description>Register a device on Pontia before opening its Dashboard.</Empty.Description>
            </Empty.Header>
            <Empty.Content>
              <Button href="https://pontia.dev">Go to Pontia</Button>
            </Empty.Content>
          </Empty.Root>
        </Card.Content>
      </Card.Root>
    {/if}
  </div>
</main>
