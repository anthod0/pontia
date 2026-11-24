<script lang="ts">
  import type { Component } from 'svelte'
  import MobileFrame from '../../../src/shell/mobile/MobileFrame.svelte'
  import { provideDashboardHost } from '../../../src/device/hostContext'
  import { createLocalHost } from '../../../src/modes/local/host.svelte'
  import { testDevice } from '../../deviceFixture'
  let { page: Page }: {page: Component} = $props()
  provideDashboardHost(createLocalHost(() => testDevice))
</script>
<MobileFrame><Page /></MobileFrame>
