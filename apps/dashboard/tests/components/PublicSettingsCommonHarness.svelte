<script lang="ts">
  import SettingsCommonPage from '../../src/modes/public/SettingsCommonPage.svelte'
  import { provideDeviceList, type DeviceList } from '../../src/modes/public/deviceListContext'

  let { list }: { list: DeviceList } = $props()
  // svelte-ignore state_referenced_locally -- the harness fixes the list for its lifetime
  provideDeviceList(list)
</script>

<SettingsCommonPage />
