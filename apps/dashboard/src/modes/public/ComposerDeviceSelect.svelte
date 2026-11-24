<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import { t } from '$lib/i18n';
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import { deviceStateLabel, type ConnectedDevice } from './deviceConnections'
  import { targetSegmentClass } from '../../components/chat/targetSegment'

  let { devices, selected, onSelect, disabled = false }: {
    devices: ConnectedDevice[]
    selected: ConnectedDevice | undefined
    onSelect: (handle: string) => void
    disabled?: boolean
  } = $props()
</script>

<ResponsiveSelect value={selected?.handle ?? ''} onValueChange={(handle) => { if (handle && handle !== selected?.handle) onSelect(handle) }} {disabled}
  label={$t('devices.device')} triggerClass={targetSegmentClass} align="start" contentClass="w-64"
  options={devices} keyOf={device => device.handle} labelOf={device => device.name}>
  {#snippet trigger()}
    <DesktopIcon class="size-3.5 text-muted-foreground" aria-hidden="true" />
    <span class="min-w-0 truncate">{selected?.name ?? $t('devices.device')}</span>
  {/snippet}
  {#snippet option(device)}
    <span class="min-w-0 flex-1 truncate">{device.name}</span>
    <span class="shrink-0 text-xs text-muted-foreground">{$t(deviceStateLabel[device.state])}</span>
  {/snippet}
</ResponsiveSelect>
