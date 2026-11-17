<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import MessageResponse from '$lib/components/ai-elements/message/message-response.svelte'
  import { highlightMarkdownCode } from '$lib/components/ai-elements/message/markdownHighlighter'
  import { readTextPreview, textFileLanguage } from '$lib/downloads/textPreview'
  import { downloads, downloadKey } from '$lib/downloads/manager'
  import { copyText } from '$lib/copyText'
  import { ApiError } from '../../api/errors'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import type { FilePreviewTarget } from './filePreviewContext'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'

  let { target, onclose }: { target: FilePreviewTarget; onclose: () => void } = $props()
  const name = $derived(target.path.slice(target.path.lastIndexOf('/') + 1))
  const language = $derived(textFileLanguage(target.path))
  const key = $derived(downloadKey(target.device.id, target.sessionId, { kind: 'path', path: target.path }))
  const task = $derived($downloads.find(task => task.key === key))
  let text = $state<string | null>(null)
  let error = $state<string | null>(null)
  let copyError = $state(false)
  let copied = $state(false)
  let copyTimer: ReturnType<typeof setTimeout> | undefined
  let attempt = $state(0)
  let view = $state('preview')
  const highlighted = $derived(text === null || (language === 'markdown' && view === 'preview') ? '' : highlightMarkdownCode(text, language))

  $effect(() => {
    const { device, sessionId, path } = target
    attempt
    const controller = new AbortController()
    text = null
    error = null
    copyError = false
    copied = false
    view = 'preview'
    void device.fileApi.pathFileContent(sessionId, path, controller.signal)
      .then(readTextPreview)
      .then(content => { if (!controller.signal.aborted) text = content })
      .catch(cause => {
        if (!controller.signal.aborted) error = cause instanceof ApiError && cause.status === 404
          ? $t('files.missing') : cause instanceof Error ? cause.message : String(cause)
      })
    return () => controller.abort()
  })

  onDestroy(() => clearTimeout(copyTimer))

  async function copy() {
    if (text === null) return
    copied = await copyText(text)
    copyError = !copied
    clearTimeout(copyTimer)
    if (copied) copyTimer = setTimeout(() => { copied = false }, 1600)
  }
</script>

{#snippet sourceContent()}
  <div class="file-source font-mono text-xs leading-relaxed">{@html highlighted}</div>
{/snippet}

<aside aria-label={$t('files.preview')} data-file-preview class="fixed inset-y-0 right-0 z-50 flex w-full min-w-0 flex-col border-l bg-surface lg:w-[var(--file-preview-width)]">
  <header class="flex shrink-0 items-center gap-2 border-b px-4 py-3">
    <div class="min-w-0 flex-1">
      <h2 class="truncate text-sm font-medium" title={target.path}>{name}</h2>
      <p class="truncate text-xs text-muted-foreground" title={target.path}>{target.path}</p>
    </div>
    <Button variant="ghost" size="icon-sm" disabled={text === null} onclick={copy} aria-label={$t('common.copy')} title={copied ? $t('common.copied') : $t('common.copy')}><CopyIcon class="size-4" /></Button>
    <Button variant="ghost" size="icon-sm" disabled={task?.status === 'receiving'} onclick={() => downloads.start(target.device.id, target.sessionId, { kind: 'path', path: target.path }, name)} aria-label={$t('files.downloadNamed', { name })} title={$t('files.download')}><DownloadSimpleIcon class="size-4" /></Button>
    <Button variant="ghost" size="icon-sm" onclick={onclose} aria-label={$t('common.close')} title={$t('common.close')}><XIcon class="size-4" /></Button>
  </header>
  {#if copied}<p role="status" class="px-4 pt-2 text-xs text-muted-foreground">{$t('common.copied')}</p>{/if}
  {#if copyError}<p role="alert" class="px-4 pt-2 text-sm text-destructive">{$t('files.copyFailed')}</p>{/if}
  {#if task?.error}<p role="alert" class="px-4 pt-2 text-sm text-destructive">{task.error}</p>{/if}
  {#if task?.status === 'receiving'}<p role="status" class="px-4 pt-2 text-xs text-muted-foreground">{$t('files.receiving')}</p>{/if}
  {#if error}
    <div class="space-y-3 p-4">
      <p role="alert" class="text-sm text-destructive">{error}</p>
      <Button variant="outline" size="sm" onclick={() => attempt++}>{$t('common.retry')}</Button>
    </div>
  {:else if text === null}
    <p role="status" class="p-4 text-sm text-muted-foreground">{$t('files.loadingPreview')}</p>
  {:else}
    {#if language === 'markdown'}
      <Tabs.Root bind:value={view} class="min-h-0 flex-1 gap-0">
        <div class="shrink-0 border-b px-4 py-2">
          <Tabs.List aria-label={$t('files.previewMode')}>
            <Tabs.Trigger value="preview">{$t('files.preview')}</Tabs.Trigger>
            <Tabs.Trigger value="source">{$t('files.source')}</Tabs.Trigger>
          </Tabs.List>
        </div>
        <Tabs.Content value="preview" class="min-h-0 flex-1 overflow-auto overscroll-contain p-4" data-file-preview-content>
          {#key target.device}
            <DeviceProvider device={target.device}>
              <MessageResponse content={text} markdown sessionId={target.sessionId} />
            </DeviceProvider>
          {/key}
        </Tabs.Content>
        <Tabs.Content value="source" class="min-h-0 flex-1 overflow-auto overscroll-contain p-4" data-file-preview-content>
          {@render sourceContent()}
        </Tabs.Content>
      </Tabs.Root>
    {:else}
      <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-4" data-file-preview-content>
        {@render sourceContent()}
      </div>
    {/if}
  {/if}
</aside>

<style>
  .file-source :global(pre.shiki) {
    min-width: 100%;
    width: max-content;
    margin: 0;
    background: transparent !important;
  }
</style>
