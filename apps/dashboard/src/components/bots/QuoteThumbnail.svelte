<script lang="ts">
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import { useDeviceRuntime } from '../../device/context'

  /** A quoted image's preview, or the file icon while it loads or when it cannot show. */
  let { sessionId, path, class: className = '' }: { sessionId: string; path: string; class?: string } = $props()
  const device = useDeviceRuntime()
  let url = $state<string | null>(null)
  $effect(() => {
    let alive = true
    void device.previews.imageUrl(sessionId, path).then(value => { if (alive) url = value }, () => {})
    return () => { alive = false }
  })
</script>

{#if url}
  <img src={url} alt="" class={['shrink-0 rounded object-cover', className]} data-quote-thumbnail />
{:else}
  <FileIcon class="size-3.5 shrink-0" />
{/if}
