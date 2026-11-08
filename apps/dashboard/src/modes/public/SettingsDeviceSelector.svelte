<script lang="ts">
  import * as Select from '$lib/components/ui/select/index.js'
  import { navigate } from '$lib/navigation'
  import { deviceStateLabel, useDeviceDirectory } from './deviceDirectory'

  let { section }: { section: string } = $props()
  const directory = useDeviceDirectory()
  const selected = $derived(directory.devices.find(device => device.handle === directory.handle))
</script>

<Select.Root type="single" value={directory.handle ?? ''} onValueChange={(handle) => {
  if (handle) void navigate(section, undefined, { scope: `/${handle}` })
}}>
  <Select.Trigger aria-label="Settings device" class="mb-5 w-full">
    <span class="truncate">{selected?.name ?? 'Choose a device'}</span>
  </Select.Trigger>
  <Select.Content>
    {#each directory.devices as device (device.handle)}
      <Select.Item value={device.handle} label={device.name}>
        {device.name} · {deviceStateLabel[device.state]}
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
