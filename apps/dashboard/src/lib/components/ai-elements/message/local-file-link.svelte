<script lang="ts">
  import { type Snippet } from 'svelte'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import { t, number } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../../device/context'

  let { sessionId, path, title, children }: {
    sessionId: string
    path: string
    title?: string | null
    children?: Snippet
  } = $props()

  const device = useDeviceRuntime()
  const name = $derived(path.slice(path.lastIndexOf('/') + 1))
  const key = $derived(downloadKey(device.id, sessionId, { kind: 'path', path }))
  const task = $derived($downloads.find(task => task.key === key))
  const status = $derived(task?.status)
  const downloading = $derived(status === 'receiving')
  let showProgress = $state(false)
  let completed = $state(false)
  const error = $derived(task?.error)
  const progress = $derived(task?.progress ?? { received: 0, total: null })

  $effect(() => {
    showProgress = false
    if (!downloading) return
    const timer = setTimeout(() => { showProgress = true }, 300)
    return () => clearTimeout(timer)
  })

  $effect(() => {
    const isComplete = status === 'complete'
    completed = isComplete
    if (!isComplete) return
    const timer = setTimeout(() => { completed = false }, 1600)
    return () => clearTimeout(timer)
  })

  const percent = $derived(progress.total === null ? undefined : progress.total === 0 ? 100 : Math.min(100, Math.floor(progress.received / progress.total * 100)))
  const received = $derived(progress.received < 1024
    ? `${$number(progress.received)} B`
    : progress.received < 1024 * 1024
      ? `${$number(progress.received / 1024, { maximumFractionDigits: 1 })} KiB`
      : `${$number(progress.received / 1024 / 1024, { maximumFractionDigits: 1 })} MiB`)

  function download() {
    downloads.start(device.id, sessionId, { kind: 'path', path }, name)
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
