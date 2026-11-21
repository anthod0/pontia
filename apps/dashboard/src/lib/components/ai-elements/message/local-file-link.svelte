<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import ImageIcon from 'phosphor-svelte/lib/ImageIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { t } from '$lib/i18n'
  import { isImagePath } from '$lib/downloads/localFilePath'
  import { downloads } from '$lib/downloads/manager'
  import { ApiError } from '../../../../api/errors'
  import { useDeviceRuntime } from '../../../../device/context'
  import { useFilePreview } from '../../../../components/chat/filePreviewContext'
  import { canPreviewText } from '../../../../components/chat/fileKind'
  import ImageLightbox from '../../../../components/chat/ImageLightbox.svelte'

  type ImageFailure = { kind: 'missing' | 'notImage' } | { kind: 'failed'; message: string }

  let { sessionId, path, anchor = null, title, children }: {
    sessionId: string
    path: string
    anchor?: string | null
    title?: string | null
    children?: Snippet
  } = $props()

  const device = useDeviceRuntime()
  const preview = useFilePreview()
  const name = $derived(path.slice(path.lastIndexOf('/') + 1))
  const image = $derived(isImagePath(path))
  // Files the preview panel cannot show, such as videos and archives, are only downloaded.
  const previewable = $derived(image || canPreviewText(name))
  let enlarged = $state(false)
  let imageUrl = $state<string | null>(null)
  let failure = $state<ImageFailure | null>(null)
  let request: AbortController | undefined

  onDestroy(() => {
    request?.abort()
    if (imageUrl) URL.revokeObjectURL(imageUrl)
  })

  $effect(() => {
    if (!enlarged) request?.abort()
  })

  function open() {
    if (image) showImage()
    else if (previewable) preview.open({ device, sessionId, path, anchor })
    else download()
  }

  function download() {
    downloads.start(device.id, sessionId, { kind: 'path', path }, name)
  }

  function showImage() {
    enlarged = true
    if (imageUrl) URL.revokeObjectURL(imageUrl)
    imageUrl = null
    failure = null
    request?.abort()
    const controller = new AbortController()
    request = controller
    void device.previews.pathImagePreview(sessionId, path, controller.signal)
      .then(blob => {
        if (controller.signal.aborted) return
        if (blob) imageUrl = URL.createObjectURL(blob)
        else failure = { kind: 'notImage' }
      })
      .catch(cause => {
        if (controller.signal.aborted) return
        failure = cause instanceof ApiError && cause.status === 404
          ? { kind: 'missing' }
          : { kind: 'failed', message: cause instanceof Error ? cause.message : String(cause) }
      })
  }
</script>

<Button
  variant="link"
  class="inline h-auto rounded-sm p-0 align-baseline font-normal whitespace-normal underline"
  title={title || path}
  aria-label={$t(image ? 'files.enlargeNamed' : previewable ? 'files.previewNamed' : 'files.downloadNamed', { name })}
  onclick={open}
>
  {@render children?.()}
  {#if image}
    <ImageIcon class="ml-1 inline-block size-3.5 align-[-0.125em]" aria-hidden="true" />
  {:else if !previewable}
    <DownloadSimpleIcon class="ml-1 inline-block size-3.5 align-[-0.125em]" aria-hidden="true" />
  {:else}
    <FileTextIcon class="ml-1 inline-block size-3.5 align-[-0.125em]" aria-hidden="true" />
  {/if}
</Button>

{#if image}
  <ImageLightbox bind:open={enlarged} images={[{ name, src: imageUrl }]}>
    {#if failure}
      <p role="alert" class="max-w-md break-words">
        {failure.kind === 'failed' ? failure.message : $t(failure.kind === 'missing' ? 'files.missing' : 'files.previewNotImage')}
      </p>
      {#if failure.kind !== 'missing'}
        <Button size="sm" variant="secondary" onclick={download}>
          <DownloadSimpleIcon data-icon="inline-start" />{$t('files.download')}
        </Button>
      {/if}
    {:else}
      <CircleNotchIcon class="size-6 animate-spin" role="status" aria-label={$t('files.loadingPreview')} />
    {/if}
  </ImageLightbox>
{/if}
