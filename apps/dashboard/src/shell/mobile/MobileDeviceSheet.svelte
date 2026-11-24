<script lang="ts">
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { useDashboardHost } from '../../device/hostContext'
  let { open = $bindable(false) }: { open: boolean } = $props()
  const host = useDashboardHost()
  const stack = useMobileStack()!
</script>

<Sheet.Root bind:open>
  <BottomSheet dismiss={() => open = false} aria-describedby={undefined}>
    <Sheet.Title class="py-3 font-semibold">{$t('devices.title')}</Sheet.Title>
    {#each host.devices as device (device.id)}
      <button class="mobile-row" aria-pressed={device.handle === host.activeDevice?.handle}
        onclick={() => stack.back(() => host.selectDevice(device.handle, {keepMobileTab: true}))}>
        {device.name}<span class="ml-auto text-muted-foreground">{$t(device.state === 'online' ? 'devices.online' : device.state === 'connecting' ? 'devices.connecting' : 'devices.offline')}</span>
      </button>
    {/each}
  </BottomSheet>
</Sheet.Root>
