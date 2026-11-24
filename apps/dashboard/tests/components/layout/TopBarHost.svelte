<script lang="ts">
  import type { Component } from 'svelte'
  import * as Sidebar from '../../../src/lib/components/ui/sidebar/index.js'
  import TopBar from '../../../src/components/layout/TopBar.svelte'
  import { provideTopBarAction } from '../../../src/components/layout/topBarAction'
  import { provideSettingsDetail } from '../../../src/components/settings/settingsDetail'
  import DeviceProvider from '../../../src/device/DeviceProvider.svelte'
  import type { DeviceRuntime } from '../../../src/device/runtime'

  let { onOpenChange, device, page: Page }: {
    onOpenChange?: (open: boolean) => void
    device?: DeviceRuntime
    page?: Component
  } = $props()
  const action = provideTopBarAction()
  provideSettingsDetail()
</script>

{#snippet content()}
  <Sidebar.Provider {onOpenChange}>
    <div data-testid="top-bar-row">
      <TopBar />
      {@render $action?.()}
    </div>
    {#if Page}<Page />{/if}
  </Sidebar.Provider>
{/snippet}

{#if device}
  <DeviceProvider {device}>{@render content()}</DeviceProvider>
{:else}
  {@render content()}
{/if}
