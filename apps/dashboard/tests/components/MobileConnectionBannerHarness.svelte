<script lang="ts">
  import { untrack } from 'svelte'
  import { provideDashboardHost, type DashboardHost } from '../../src/device/hostContext'
  import MobileConnectionBanner from '../../src/shell/mobile/MobileConnectionBanner.svelte'
  let { host }: { host: DashboardHost } = $props()
  provideDashboardHost(untrack(() => host))
</script>

<MobileConnectionBanner />
