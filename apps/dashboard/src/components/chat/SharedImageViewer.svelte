<script lang="ts">
  import { t } from '$lib/i18n'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { useDeviceRuntime } from '../../device/context'
  import { ApiError } from '../../api/errors'
  import type { SharedFile } from '../../api/types'
  import ImageLightbox from './ImageLightbox.svelte'

  /** Pages through `files`, which must all pass `canPreviewImage`, loading the active image and its neighbours. */
  let { sessionId, files, open = $bindable(false), index = $bindable(0) }: {
    sessionId: string
    files: SharedFile[]
    open?: boolean
    index?: number
  } = $props()

  const device = useDeviceRuntime()
  let urls = $state<Record<string, string | null>>({})
  let errors = $state<Record<string, string>>({})
  const requested = new Set<string>()
  const images = $derived(files.map(file => ({ name: file.name, src: urls[file.file_id] ?? null })))
  const active = $derived(files[index])

  $effect(() => {
    if (!open) return
    for (const file of files.slice(Math.max(0, index - 1), index + 2)) load(file)
  })

  function load(file: SharedFile) {
    if (requested.has(file.file_id)) return
    requested.add(file.file_id)
    void device.previews.sharedImageUrl(sessionId, file.file_id).then(url => {
      urls[file.file_id] = url
      if (!url) errors[file.file_id] = $t('files.previewNotImage')
    }).catch(cause => {
      requested.delete(file.file_id)
      errors[file.file_id] = cause instanceof ApiError && cause.status === 404
        ? $t('files.missing')
        : cause instanceof Error ? cause.message : String(cause)
    })
  }
</script>

{#if active}
  <ImageLightbox bind:open bind:index {images} label={$t('files.sharedImage')}>
    {#if errors[active.file_id]}
      <p role="alert" class="max-w-md break-words">{errors[active.file_id]}</p>
    {:else}
      <CircleNotchIcon class="size-6 animate-spin" role="status" aria-label={$t('files.loadingPreview')} />
    {/if}
  </ImageLightbox>
{/if}
