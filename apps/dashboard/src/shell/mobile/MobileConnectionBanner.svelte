<script lang="ts">
  import { useDashboardHost } from '../../device/hostContext'
  import { readable } from 'svelte/store'
  import { t } from '$lib/i18n'
  const host = useDashboardHost()
  const stream = $derived(host.activeDevice?.runtime?.connection.sseStatus ?? readable(null))
  const offline = $derived(host.activeDevice?.state === 'offline')
  const unhealthy = $derived(offline || host.connecting || $stream === 'error' || $stream === 'reconnecting' || $stream === 'connecting')
  let visible = $state(false)
  $effect(() => {
    if (!unhealthy) { visible = false; return }
    const timer = setTimeout(() => visible = true, offline ? 0 : 3000)
    return () => clearTimeout(timer)
  })
</script>

{#if visible}
  <div role="status" class="flex items-center justify-between gap-2 bg-muted px-4 text-xs">
    {$t('connection.lost')}<button class="min-h-9 text-primary" onclick={host.retryActiveOrRefresh}>{$t('common.retry')}</button>
  </div>
{/if}
