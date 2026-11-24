<script lang="ts">
  import SettingsCommonPage from '../../src/modes/public/SettingsCommonPage.svelte'
  import { provideDashboardHost, type HostDevice } from '../../src/device/hostContext'
  import { createLocalHost } from '../../src/modes/local/host.svelte'
  import { testDevice } from '../deviceFixture'

  let { devices, retryDevice }: { devices: HostDevice[]; retryDevice: (handle: string) => void } = $props()
  provideDashboardHost({
    ...createLocalHost(() => testDevice), mode: 'public',
    get devices() { return devices }, retryDevice: handle => retryDevice(handle),
  })
</script>

<SettingsCommonPage />
