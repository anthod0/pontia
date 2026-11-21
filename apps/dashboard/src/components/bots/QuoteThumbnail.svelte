<script lang="ts">
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import { useDeviceRuntime } from '../../device/context'
  import type { SharedFile } from '../../api/types'

  /** A quoted shared image's preview, or the file icon while it loads or when it cannot show. */
  let { sessionId, file, class: className = '' }: { sessionId: string; file: SharedFile; class?: string } = $props()
  const device = useDeviceRuntime()
  let url = $state<string | null>(null)
  $effect(() => {
    let alive = true
    void device.previews.sharedImageUrl(sessionId, file.file_id).then(value => { if (alive) url = value }, () => {})
    return () => { alive = false }
  })
</script>

{#if url}
  <img src={url} alt="" class={['shrink-0 rounded object-cover', className]} data-quote-thumbnail />
{:else}
  <FileIcon class="size-3.5 shrink-0" />
{/if}
