<script lang="ts">
  import ResponsiveMenu from '$lib/components/ui/overlay/ResponsiveMenu.svelte'
  import { t } from '$lib/i18n';
  import ArrowsLeftRightIcon from 'phosphor-svelte/lib/ArrowsLeftRightIcon'
  import CaretUpDownIcon from 'phosphor-svelte/lib/CaretUpDownIcon'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { shortcuts } from '$lib/shortcuts/store.svelte'
  import type { HostDevice } from '../../device/hostContext'
  import { deviceStateDotClass, deviceStateLabel } from '../../components/devices/deviceState'

  let { devices, selected, previous, onSelect }: {
    devices: HostDevice[]
    selected: HostDevice | null
    previous: HostDevice | null
    onSelect: (handle: string) => void
  } = $props()
</script>

<Sidebar.Group class="shrink-0 pb-0 group-data-[collapsible=icon]:hidden">
  <Sidebar.Menu>
    <Sidebar.MenuItem>
      <ResponsiveMenu label={$t('devices.title')} align="start" class="w-(--bits-dropdown-menu-anchor-width) min-w-56">
        {#snippet trigger()}
          <Sidebar.MenuButton class="text-[13px]" aria-label={$t('devices.sidebarDevice')} title={selected ? $t(deviceStateLabel[selected.state]) : undefined}>
            {#snippet child({ props })}
              <DropdownMenu.Trigger {...props}>
                <DesktopIcon class="text-sidebar-foreground/60" />
                <span class="min-w-0 truncate font-medium">{selected?.name ?? $t('devices.noDevice')}</span>
                {#if selected}
                  <span class={['size-1.5 shrink-0 rounded-full', deviceStateDotClass[selected.state]]} aria-label={$t(deviceStateLabel[selected.state])}></span>
                {/if}
                <span class="flex-1"></span>
                <CaretUpDownIcon class="text-sidebar-foreground/60" />
              </DropdownMenu.Trigger>
            {/snippet}
          </Sidebar.MenuButton>
        {/snippet}
        <DropdownMenu.Label class="pl-7">{$t('devices.title')}</DropdownMenu.Label>
        <DropdownMenu.RadioGroup value={selected?.handle ?? ''} onValueChange={onSelect}>
          {#each devices as device (device.handle)}
            <DropdownMenu.RadioItem value={device.handle} class="pr-2 pl-7 *:data-[slot=dropdown-menu-radio-item-indicator]:right-auto *:data-[slot=dropdown-menu-radio-item-indicator]:left-2">
              <span class="min-w-0 flex-1 truncate">{device.name}</span>
              <span class="shrink-0 text-xs text-muted-foreground">{$t(deviceStateLabel[device.state])}</span>
            </DropdownMenu.RadioItem>
          {/each}
        </DropdownMenu.RadioGroup>
      </ResponsiveMenu>
      {#if previous}
        <Sidebar.MenuAction
          aria-label={$t('devices.switchBack', { name: previous.name })}
          title={shortcuts.label('previousDevice') ? $t('devices.switchBackTitle', { name: previous.name, state: $t(deviceStateLabel[previous.state]), keys: shortcuts.label('previousDevice') }) : $t('devices.switchBack', { name: previous.name })}
          onclick={() => onSelect(previous.handle)}
        >
          <ArrowsLeftRightIcon />
        </Sidebar.MenuAction>
      {/if}
    </Sidebar.MenuItem>
  </Sidebar.Menu>
</Sidebar.Group>
