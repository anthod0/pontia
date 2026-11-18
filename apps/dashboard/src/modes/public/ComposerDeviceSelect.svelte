<script lang="ts">
  import { t } from '$lib/i18n';
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import * as Select from '$lib/components/ui/select/index.js'
  import { deviceStateLabel, type ConnectedDevice } from './deviceConnections'
  import { targetSegmentClass } from '../../components/chat/targetSegment'

  let { devices, selected, onSelect, disabled = false }: {
    devices: ConnectedDevice[]
    selected: ConnectedDevice | undefined
    onSelect: (handle: string) => void
    disabled?: boolean
  } = $props()
</script>

<Select.Root type="single" value={selected?.handle ?? ''} onValueChange={(handle) => { if (handle && handle !== selected?.handle) onSelect(handle) }} {disabled}>
  <Select.Trigger class={targetSegmentClass} aria-label={$t('devices.device')}>
    <DesktopIcon class="size-3.5 text-muted-foreground" aria-hidden="true" />
    <span class="min-w-0 truncate">{selected?.name ?? $t('devices.device')}</span>
  </Select.Trigger>
  <Select.Content align="start" class="w-64">
    {#each devices as device (device.handle)}
      <Select.Item value={device.handle} label={device.name}>
        <span class="min-w-0 flex-1 truncate">{device.name}</span>
        <span class="shrink-0 text-xs text-muted-foreground">{$t(deviceStateLabel[device.state])}</span>
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
