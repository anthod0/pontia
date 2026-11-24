<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import MessageResponse from '$lib/components/ai-elements/message/message-response.svelte'
  import { highlightMarkdownCode } from '$lib/components/ai-elements/message/markdownHighlighter'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { readTextPreview, textFileLanguage, TextPreviewError } from '$lib/downloads/textPreview'
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import { copyText } from '$lib/copyText'
  import { ApiError } from '../../api/errors'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import { provideFilePreviewNavigator, type FilePreviewTarget } from './filePreviewContext'
  import { toast } from 'svelte-sonner'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import BinaryIcon from 'phosphor-svelte/lib/BinaryIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import CodeIcon from 'phosphor-svelte/lib/CodeIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import EyeIcon from 'phosphor-svelte/lib/EyeIcon'
  import FileArrowDownIcon from 'phosphor-svelte/lib/FileArrowDownIcon'
  import FileDashedIcon from 'phosphor-svelte/lib/FileDashedIcon'
  import FileXIcon from 'phosphor-svelte/lib/FileXIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'

  /** One file in the panel's navigation history, with the reading position to restore on return. */
  type HistoryEntry = { path: string; anchor: string | null; view: string; scrollTop: number }
  type PreviewFailure = { kind: 'missing' | 'tooLarge' | 'notText' } | { kind: 'failed'; message: string }

  const failureStates = {
    missing: { icon: FileXIcon, title: 'files.missing', description: 'files.previewMissingDescription' },
    tooLarge: { icon: FileArrowDownIcon, title: 'files.previewTooLargeTitle', description: 'files.previewTooLarge' },
    notText: { icon: BinaryIcon, title: 'files.previewNotTextTitle', description: 'files.previewNotText' },
    failed: { icon: WarningCircleIcon, title: 'files.previewFailedTitle', description: null },
  } as const
  const skeletonLines = [42, 68, 55, 0, 74, 61, 83, 36, 0, 58, 70, 47, 64, 29]

  let { target, onclose }: { target: FilePreviewTarget; onclose: () => void } = $props()
  // The panel is recreated for each target opened from outside, so history starts from the initial target.
  // svelte-ignore state_referenced_locally
  let history = $state<HistoryEntry[]>([{ path: target.path, anchor: target.anchor ?? null, view: 'preview', scrollTop: 0 }])
  const current = $derived(history[history.length - 1])
  const path = $derived(current.path)
  const name = $derived(path.slice(path.lastIndexOf('/') + 1))
  const language = $derived(textFileLanguage(path))
  const htmlPreview = $derived(language === 'html' && name.includes('.'))
  const hasPreview = $derived(language === 'markdown' || htmlPreview)
  const key = $derived(downloadKey(target.device.id, target.sessionId, { path }))
  const task = $derived($downloads.find(task => task.key === key))
  const receiving = $derived(task?.status === 'receiving')
  let text = $state<string | null>(null)
  let failure = $state<PreviewFailure | null>(null)
  let copied = $state(false)
  let copyTimer: ReturnType<typeof setTimeout> | undefined
  let view = $state('preview')
  let panel: HTMLElement
  const highlighted = $derived(text === null || (hasPreview && view === 'preview') ? '' : highlightMarkdownCode(text, language))

  provideFilePreviewNavigator({
    open(next) {
      if (next.device !== target.device || next.sessionId !== target.sessionId) return
      if (next.path === path) {
        if (next.anchor) void scrollToAnchor(next.anchor)
        return
      }
      history[history.length - 1] = { ...current, view, scrollTop: scroller()?.scrollTop ?? 0 }
      history.push({ path: next.path, anchor: next.anchor ?? null, view: 'preview', scrollTop: 0 })
    },
  })

  $effect(() => {
    const { device, sessionId } = target
    const entry = current
    const controller = new AbortController()
    text = null
    failure = null
    copied = false
    view = entry.anchor ? 'preview' : entry.view
    void device.fileApi.pathFileContent(sessionId, entry.path, controller.signal)
      .then(readTextPreview)
      .then(async content => {
        if (controller.signal.aborted) return
        text = content
        await tick()
        if (controller.signal.aborted) return
        if (entry.anchor) await scrollToAnchor(entry.anchor)
        else if (entry.scrollTop) scroller()?.scrollTo({ top: entry.scrollTop })
      })
      .catch(cause => {
        if (!controller.signal.aborted) failure = previewFailure(cause)
      })
    return () => controller.abort()
  })

  onDestroy(() => clearTimeout(copyTimer))

  function scroller() {
    return panel.querySelector<HTMLElement>('[data-file-preview-content]:not([hidden])')
  }

  async function scrollToAnchor(anchor: string) {
    if (language !== 'markdown') return
    view = 'preview'
    await tick()
    const heading = [...scroller()?.querySelectorAll('[id]') ?? []].find(element => element.id === anchor)
    heading?.scrollIntoView({ block: 'start' })
  }

  function back() {
    history.pop()
  }

  function refresh() {
    const content = scroller()
    history[history.length - 1] = content
      ? { path, anchor: null, view, scrollTop: content.scrollTop }
      : { ...current }
  }

  async function copy() {
    if (text === null) return
    copied = await copyText(text)
    clearTimeout(copyTimer)
    if (copied) copyTimer = setTimeout(() => { copied = false }, 1600)
    else toast.error($t('files.copyFailed'))
  }

  function download() {
    downloads.start(target.device.id, target.sessionId, { path }, name)
  }

  function previewFailure(cause: unknown): PreviewFailure {
    if (cause instanceof ApiError && cause.status === 404) return { kind: 'missing' }
    if (cause instanceof TextPreviewError) return { kind: cause.reason }
    return { kind: 'failed', message: cause instanceof Error ? cause.message : String(cause) }
  }
</script>

{#snippet sourceContent()}
  <div class="file-source font-mono text-xs leading-relaxed">{@html highlighted}</div>
{/snippet}

<div bind:this={panel} aria-label={$t('files.preview')} data-file-preview class="flex min-h-0 min-w-0 flex-1 flex-col">
  <Tabs.Root bind:value={view} class="min-h-0 flex-1 gap-0">
    <header class="flex shrink-0 items-center gap-1 border-b px-2 py-2">
      {#if history.length > 1}
        <Button variant="ghost" size="icon-sm" onclick={back} aria-label={$t('files.previewBack')} title={$t('files.previewBack')}><ArrowLeftIcon class="size-4" /></Button>
      {/if}
      <h2 class="min-w-0 flex-1 truncate text-sm font-medium" title={name}>{name}</h2>
      {#if hasPreview && text}
        <Tabs.List aria-label={$t('files.previewMode')} class="shrink-0">
          <Tabs.Trigger value="preview" aria-label={$t('files.preview')} title={$t('files.preview')}><EyeIcon class="size-4" /></Tabs.Trigger>
          <Tabs.Trigger value="source" aria-label={$t('files.source')} title={$t('files.source')}><CodeIcon class="size-4" /></Tabs.Trigger>
        </Tabs.List>
      {/if}
      <Button variant="ghost" size="icon-sm" disabled={text === null && !failure} onclick={refresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}><ArrowClockwiseIcon class="size-4" /></Button>
      <Button variant="ghost" size="icon-sm" disabled={text === null} onclick={copy} aria-label={$t('common.copy')} title={copied ? $t('common.copied') : $t('common.copy')}>
        {#if copied}<CheckIcon class="size-4 text-primary" />{:else}<CopyIcon class="size-4" />{/if}
      </Button>
      <Button variant="ghost" size="icon-sm" disabled={receiving} onclick={download} aria-label={$t('files.downloadNamed', { name })} title={receiving ? $t('files.receiving') : $t('files.download')}>
        {#if receiving}<CircleNotchIcon class="size-4 animate-spin" />{:else}<DownloadSimpleIcon class="size-4" />{/if}
      </Button>
      <Button variant="ghost" size="icon-sm" onclick={onclose} aria-label={$t('files.closeFile')} title={$t('files.closeFile')}><XIcon class="size-4" /></Button>
    </header>
    <span class="sr-only" role="status">{copied ? $t('common.copied') : receiving ? $t('files.receiving') : ''}</span>
    {#if task?.error}
      <Alert.Root variant="destructive" class="mx-4 mt-3 w-auto">
        <WarningCircleIcon />
        <Alert.Description class="break-words">{task.error}</Alert.Description>
      </Alert.Root>
    {/if}
    {#if failure}
      {@const state = failureStates[failure.kind]}
      <Empty.Root role="alert" class="min-h-0 p-8">
        <Empty.Header>
          <Empty.Media class={['size-12 rounded-xl', failure.kind === 'missing' || failure.kind === 'failed' ? 'bg-destructive/10 text-destructive' : 'bg-muted text-muted-foreground']}>
            <state.icon class="size-6" />
          </Empty.Media>
          <Empty.Title>{$t(state.title)}</Empty.Title>
          <Empty.Description class="break-words">{state.description ? $t(state.description) : failure.kind === 'failed' && failure.message}</Empty.Description>
        </Empty.Header>
        <Empty.Content>
          {#if failure.kind === 'tooLarge' || failure.kind === 'notText'}
            <Button size="sm" disabled={receiving} onclick={download}><DownloadSimpleIcon data-icon="inline-start" />{$t('files.download')}</Button>
          {:else}
            <Button variant="outline" size="sm" onclick={refresh}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
          {/if}
        </Empty.Content>
      </Empty.Root>
    {:else if text === null}
      <div role="status" aria-label={$t('files.loadingPreview')} class="min-h-0 flex-1 space-y-2.5 overflow-hidden p-4">
        {#each skeletonLines as width, index (index)}
          <Skeleton class="h-3 rounded-sm" style="width: {width}%" />
        {/each}
      </div>
    {:else if text === ''}
      <Empty.Root class="min-h-0 p-8">
        <Empty.Header>
          <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileDashedIcon class="size-6" /></Empty.Media>
          <Empty.Title>{$t('files.emptyFile')}</Empty.Title>
        </Empty.Header>
      </Empty.Root>
    {:else if hasPreview}
      <Tabs.Content value="preview" class={['min-h-0 flex-1', htmlPreview ? 'overflow-hidden' : 'overflow-auto overscroll-contain p-4']} data-file-preview-content>
        {#if htmlPreview}
          {#if view === 'preview'}
            <iframe title={$t('files.previewNamed', { name })} srcdoc={text} sandbox="allow-scripts allow-forms allow-modals" class="block size-full border-0 bg-white"></iframe>
          {/if}
        {:else}
          {#key target.device}
            <DeviceProvider device={target.device}>
              <MessageResponse content={text} markdown sessionId={target.sessionId} linkBase={path} class="h-auto" />
            </DeviceProvider>
          {/key}
        {/if}
      </Tabs.Content>
      <Tabs.Content value="source" class="min-h-0 flex-1 overflow-auto overscroll-contain p-4" data-file-preview-content>
        {@render sourceContent()}
      </Tabs.Content>
    {:else}
      <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-4" data-file-preview-content>
        {@render sourceContent()}
      </div>
    {/if}
  </Tabs.Root>
</div>

<style>
  .file-source :global(pre.shiki) {
    min-width: 100%;
    width: max-content;
    margin: 0;
    background: transparent !important;
  }
</style>
