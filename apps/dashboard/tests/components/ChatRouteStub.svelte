<script lang="ts">
  import { rememberLastPage } from '../../src/lib/lastPage'
  import { useDeviceRuntime } from '../../src/device/context'

  // Records the opened Session chat like the chat route does.
  let { sessionId }: { sessionId: string } = $props()
  const device = useDeviceRuntime()
  $effect(() => rememberLastPage(device.id, { kind: 'chat', id: sessionId }))
</script>

<p>Device page</p>
