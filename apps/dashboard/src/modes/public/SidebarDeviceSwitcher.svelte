<script lang="ts">
  import CaretUpDownIcon from 'phosphor-svelte/lib/CaretUpDownIcon'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { deviceStateLabel, type ConnectedDevice } from './deviceConnections'

  let { devices, selected, onSelect }: {
    devices: ConnectedDevice[]
    selected: ConnectedDevice | undefined
    onSelect: (handle: string) => void
  } = $props()
</script>

<Sidebar.Group class="shrink-0 pb-0 group-data-[collapsible=icon]:hidden">
  <Sidebar.Menu>
    <Sidebar.MenuItem>
      <DropdownMenu.Root>
        <Sidebar.MenuButton aria-label="Sidebar device" title={selected ? deviceStateLabel[selected.state] : undefined}>
          {#snippet child({ props })}
            <DropdownMenu.Trigger {...props}>
              <DesktopIcon class="text-sidebar-foreground/60" />
              <span class="min-w-0 flex-1 truncate font-medium">{selected?.name ?? 'No device'}</span>
              <CaretUpDownIcon class="text-sidebar-foreground/60" />
            </DropdownMenu.Trigger>
          {/snippet}
        </Sidebar.MenuButton>
        <DropdownMenu.Content align="start" class="w-(--bits-dropdown-menu-anchor-width) min-w-56">
          <DropdownMenu.Label class="pl-7">Devices</DropdownMenu.Label>
          <DropdownMenu.RadioGroup value={selected?.handle ?? ''} onValueChange={onSelect}>
            {#each devices as device (device.handle)}
              <DropdownMenu.RadioItem value={device.handle} class="pr-2 pl-7 *:data-[slot=dropdown-menu-radio-item-indicator]:right-auto *:data-[slot=dropdown-menu-radio-item-indicator]:left-2">
                <span class="min-w-0 flex-1 truncate">{device.name}</span>
                <span class="shrink-0 text-xs text-muted-foreground">{deviceStateLabel[device.state]}</span>
              </DropdownMenu.RadioItem>
            {/each}
          </DropdownMenu.RadioGroup>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </Sidebar.MenuItem>
  </Sidebar.Menu>
</Sidebar.Group>
