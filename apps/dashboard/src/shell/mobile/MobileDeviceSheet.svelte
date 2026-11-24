<script lang="ts">
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { useDashboardHost } from '../../device/hostContext'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import { deviceStateLabel, deviceStateDotClass } from '../../components/devices/deviceState'
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
        <DesktopIcon /><span class="min-w-0 flex-1 truncate text-left">{device.name}</span>
        <span class={['size-2 shrink-0 rounded-full',deviceStateDotClass[device.state]]}></span>
        <small>{$t(deviceStateLabel[device.state])}</small>
        {#if device.handle === host.activeDevice?.handle}<CheckIcon class="text-primary" />{/if}
      </button>
    {/each}
  </BottomSheet>
</Sheet.Root>
