<script lang="ts">
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import { canPreviewImage } from '$lib/downloads/preview'
  import { useDeviceRuntime } from '../../device/context'

  /** A quoted image's preview, or the file icon while it loads or when it cannot show. */
  let { sessionId, path, class: className = '', lazy = false }: { sessionId: string; path: string; class?: string; lazy?: boolean } = $props()
  const device = useDeviceRuntime()
  let url = $state<string | null>(null)
  let visible = $state(false)

  function observe(node: HTMLElement) {
    if (!lazy) {
      visible = true
      return
    }
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some(entry => entry.isIntersecting)) return
      visible = true
      observer.disconnect()
    })
    observer.observe(node)
    return { destroy: () => observer.disconnect() }
  }

  $effect(() => {
    if (!visible) return
    let alive = true
    const preview = lazy
      ? device.fileApi.pathFileInfo(sessionId, path)
        .then(info => canPreviewImage(info) ? device.previews.imageUrl(sessionId, path) : null)
      : device.previews.imageUrl(sessionId, path)
    void preview.then(value => { if (alive) url = value }, () => {})
    return () => { alive = false }
  })
</script>

<span use:observe class={['grid shrink-0 place-items-center', className]}>
  {#if url}
    <img src={url} alt="" class="size-full rounded object-cover" data-quote-thumbnail />
  {:else}
    <FileIcon class="size-3.5 shrink-0" />
  {/if}
</span>
