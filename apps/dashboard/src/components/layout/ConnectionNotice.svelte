<script lang="ts">
  import { useDeviceRuntime } from '../../device/context';
  import { toast } from 'svelte-sonner'
  const device = useDeviceRuntime();
  const { sseStatus } = device.connection;

  const graceMs = 3000
  const toastId = 'connection-lost'

  const unhealthy = $derived($sseStatus === 'connecting' || $sseStatus === 'reconnecting' || $sseStatus === 'error')

  $effect(() => {
    if (!unhealthy) return
    const timer = setTimeout(() => {
      toast.warning('Connection lost, reconnecting…', {
        id: toastId,
        duration: Number.POSITIVE_INFINITY,
        dismissible: false,
      })
    }, graceMs)
    return () => {
      clearTimeout(timer)
      toast.dismiss(toastId)
    }
  })
</script>
