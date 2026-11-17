<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { saveDownload, type DownloadProgress } from '$lib/downloads/save'
  import { t, number } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../../device/context'
  import { ApiError } from '../../../../api/errors'

  let { sessionId, path, title, children }: {
    sessionId: string
    path: string
    title?: string | null
    children?: Snippet
  } = $props()

  const device = useDeviceRuntime()
  const name = $derived(path.slice(path.lastIndexOf('/') + 1))
  let downloading = $state(false)
  let showProgress = $state(false)
  let completed = $state(false)
  let error = $state<string | null>(null)
  let progress = $state<DownloadProgress>({ received: 0, total: null })
  let progressTimer: ReturnType<typeof setTimeout> | undefined
  let completedTimer: ReturnType<typeof setTimeout> | undefined
  let request: AbortController | undefined

  onDestroy(() => {
    clearTimeout(progressTimer)
    clearTimeout(completedTimer)
    request?.abort()
  })

  const percent = $derived(progress.total === null ? undefined : progress.total === 0 ? 100 : Math.min(100, Math.floor(progress.received / progress.total * 100)))
  const received = $derived(progress.received < 1024
    ? `${$number(progress.received)} B`
    : progress.received < 1024 * 1024
      ? `${$number(progress.received / 1024, { maximumFractionDigits: 1 })} KiB`
      : `${$number(progress.received / 1024 / 1024, { maximumFractionDigits: 1 })} MiB`)

  async function download() {
    if (downloading) return
    downloading = true
    completed = false
    error = null
    progress = { received: 0, total: null }
    request = new AbortController()
    clearTimeout(completedTimer)
    progressTimer = setTimeout(() => { showProgress = true }, 300)
    try {
      const response = await device.fileApi.pathFileContent(sessionId, path, request.signal)
      await saveDownload(response, name, value => { progress = value })
      completed = true
      completedTimer = setTimeout(() => { completed = false }, 1600)
    } catch (cause) {
      if (!request.signal.aborted) {
        error = cause instanceof ApiError && cause.status === 404
          ? $t('files.missing')
          : cause instanceof Error ? cause.message : String(cause)
      }
    } finally {
      clearTimeout(progressTimer)
      showProgress = false
      downloading = false
    }
  }
</script>

<Button
  variant="link"
  class="inline h-auto rounded-sm p-0 align-baseline font-normal whitespace-normal underline"
  title={title || path}
  aria-label={$t(downloading ? 'files.receivingNamed' : 'files.downloadNamed', { name })}
  aria-busy={downloading}
  disabled={downloading}
  onclick={download}
>
  {@render children?.()}
  {#if showProgress}
    <CircleNotchIcon class="ml-1 inline-block size-3.5 animate-spin align-[-0.125em]" aria-hidden="true" />
  {:else if completed}
    <CheckIcon class="ml-1 inline-block size-3.5 align-[-0.125em]" aria-hidden="true" />
  {:else}
    <DownloadSimpleIcon class="ml-1 inline-block size-3.5 align-[-0.125em]" aria-hidden="true" />
  {/if}
</Button>
{#if showProgress}
  <span class="ml-1 whitespace-nowrap text-xs text-muted-foreground" role="progressbar" aria-label={$t('files.receptionProgress')} aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}>
    {percent === undefined ? received : `${$number(percent)}%`}
  </span>
{/if}
{#if error}
  <span class="ml-1 text-xs text-destructive" role="alert">{error}</span>
{/if}
