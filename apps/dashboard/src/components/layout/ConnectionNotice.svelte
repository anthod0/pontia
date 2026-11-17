<script lang="ts">
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import { toast } from 'svelte-sonner'
  const device = useDeviceRuntime();
  const { sseStatus } = device.connection;

  const graceMs = 3000
  const toastId = 'connection-lost'

  const unhealthy = $derived($sseStatus === 'connecting' || $sseStatus === 'reconnecting' || $sseStatus === 'error')

  $effect(() => {
    if (!unhealthy) return
    const message = $t('connection.lost')
    const timer = setTimeout(() => {
      toast.warning(message, {
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
