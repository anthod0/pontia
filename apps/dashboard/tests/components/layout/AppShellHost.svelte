<script lang="ts">
  import AppShell from '../../../src/shell/AppShell.svelte'
  import { provideDashboardHost } from '../../../src/device/hostContext'
  import { createLocalHost } from '../../../src/modes/local/host.svelte'
  import { testDevice } from '../../deviceFixture'
  provideDashboardHost(createLocalHost(() => testDevice))
</script>

<AppShell>
  <p>App shell page content</p>
</AppShell>
