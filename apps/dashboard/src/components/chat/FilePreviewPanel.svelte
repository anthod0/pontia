<script lang="ts">
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import MessageResponse from '$lib/components/ai-elements/message/message-response.svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import DelimitedTablePreview from './DelimitedTablePreview.svelte'
  import DeviceProvider from '../../device/DeviceProvider.svelte'
  import { type FilePreviewTarget } from './filePreviewContext'
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

import {createFilePreview} from '../../features/chat/filePreview.svelte'
import {untrack} from 'svelte'
  const failureStates = {
    missing: { icon: FileXIcon, title: 'files.missing', description: 'files.previewMissingDescription' },
    tooLarge: { icon: FileArrowDownIcon, title: 'files.previewTooLargeTitle', description: 'files.previewTooLarge' },
    notText: { icon: BinaryIcon, title: 'files.previewNotTextTitle', description: 'files.previewNotText' },
    failed: { icon: WarningCircleIcon, title: 'files.previewFailedTitle', description: null },
  } as const
  const skeletonLines = [42, 68, 55, 0, 74, 61, 83, 36, 0, 58, 70, 47, 64, 29]

  let { target, onclose, openFile }: { target: FilePreviewTarget; onclose: () => void; openFile?: (next:FilePreviewTarget)=>void } = $props()
const preview = untrack(() => createFilePreview(target, openFile))
</script>

{#snippet actions()}
      <Button variant="ghost" size="icon-sm" class={openFile ? "min-h-11 w-auto gap-2 px-4" : ""} disabled={preview.text === null} onclick={preview.copy} aria-label={$t('common.copy')} title={preview.copied ? $t('common.copied') : $t('common.copy')}>
        {#if preview.copied}<CheckIcon class="size-4 text-primary" />{:else}<CopyIcon class="size-4" />{/if}
      {#if openFile}{$t('common.copy')}{/if}</Button>
      <Button variant="ghost" size="icon-sm" class={openFile ? "min-h-11 w-auto gap-2 px-4" : ""} disabled={preview.receiving} onclick={preview.download} aria-label={$t('files.downloadNamed', {name: preview.name})} title={preview.receiving ? $t('files.receiving') : $t('files.download')}>
        {#if preview.receiving}<CircleNotchIcon class="size-4 animate-spin" />{:else}<DownloadSimpleIcon class="size-4" />{/if}{#if openFile}{$t('files.download')}{/if}
      </Button>
{/snippet}
{#snippet sourceContent()}
  <div class="file-source font-mono text-xs leading-relaxed">{@html preview.highlighted}</div>
{/snippet}

<div bind:this={preview.panel} aria-label={$t('files.preview')} data-file-preview class="flex min-h-0 min-w-0 flex-1 flex-col">
  <Tabs.Root bind:value={preview.view} class="min-h-0 flex-1 gap-0">
    <header class="flex shrink-0 items-center gap-1 border-b px-2 py-2">
      {#if preview.canGoBack}
        <Button variant="ghost" size="icon-sm" onclick={preview.back} aria-label={$t('files.previewBack')} title={$t('files.previewBack')}><ArrowLeftIcon class="size-4" /></Button>
      {/if}
      <h2 class="min-w-0 flex-1 truncate text-sm font-medium" title={preview.name}>{preview.name}</h2>
      {#if preview.hasPreview && preview.text}
        <Tabs.List aria-label={$t('files.previewMode')} class="shrink-0">
          <Tabs.Trigger value="preview" aria-label={$t('files.preview')} title={$t('files.preview')}><EyeIcon class="size-4" /></Tabs.Trigger>
          <Tabs.Trigger value="source" aria-label={$t('files.source')} title={$t('files.source')}><CodeIcon class="size-4" /></Tabs.Trigger>
        </Tabs.List>
      {/if}
      <Button variant="ghost" size="icon-sm" disabled={preview.text === null && !preview.failure} onclick={preview.refresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}><ArrowClockwiseIcon class="size-4" /></Button>
      {#if !openFile}{@render actions()}
      <Button variant="ghost" size="icon-sm" onclick={onclose} aria-label={$t('files.closeFile')} title={$t('files.closeFile')}><XIcon class="size-4" /></Button>{/if}
    </header>
    <span class="sr-only" role="status">{preview.copied ? $t('common.copied') : preview.receiving ? $t('files.receiving') : ''}</span>
    {#if preview.task?.error}
      <Alert.Root variant="destructive" class="mx-4 mt-3 w-auto">
        <WarningCircleIcon />
        <Alert.Description class="break-words">{preview.task.error}</Alert.Description>
      </Alert.Root>
    {/if}
    {#if preview.failure}
      {@const state = failureStates[preview.failure.kind]}
      <Empty.Root role="alert" class="min-h-0 p-8">
        <Empty.Header>
          <Empty.Media class={['size-12 rounded-xl', preview.failure.kind === 'missing' || preview.failure.kind === 'failed' ? 'bg-destructive/10 text-destructive' : 'bg-muted text-muted-foreground']}>
            <state.icon class="size-6" />
          </Empty.Media>
          <Empty.Title>{$t(state.title)}</Empty.Title>
          <Empty.Description class="break-words">{state.description ? $t(state.description) : preview.failure.kind === 'failed' && preview.failure.message}</Empty.Description>
        </Empty.Header>
        <Empty.Content>
          {#if preview.failure.kind === 'tooLarge' || preview.failure.kind === 'notText'}
            <Button size="sm" disabled={preview.receiving} onclick={preview.download}><DownloadSimpleIcon data-icon="inline-start" />{$t('files.download')}</Button>
          {:else}
            <Button variant="outline" size="sm" onclick={preview.refresh}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
          {/if}
        </Empty.Content>
      </Empty.Root>
    {:else if preview.text === null}
      <div role="status" aria-label={$t('files.loadingPreview')} class="min-h-0 flex-1 space-y-2.5 overflow-hidden p-4">
        {#each skeletonLines as width, index (index)}
          <Skeleton class="h-3 rounded-sm" style="width: {width}%" />
        {/each}
      </div>
    {:else if preview.text === ''}
      <Empty.Root class="min-h-0 p-8">
        <Empty.Header>
          <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileDashedIcon class="size-6" /></Empty.Media>
          <Empty.Title>{$t('files.emptyFile')}</Empty.Title>
        </Empty.Header>
      </Empty.Root>
    {:else if preview.hasPreview}
      <Tabs.Content value="preview" class={['min-h-0 flex-1', preview.htmlPreview ? 'overflow-hidden' : preview.delimiter ? 'overflow-auto overscroll-contain' : 'overflow-auto overscroll-contain p-4']} data-file-preview-content>
        {#if preview.htmlPreview}
          {#if preview.view === 'preview'}
            <iframe title={$t('files.previewNamed', {name: preview.name})} srcdoc={preview.text} sandbox="allow-scripts allow-forms allow-modals" class="block size-full border-0 bg-white"></iframe>
          {/if}
        {:else if preview.delimiter}
          <DelimitedTablePreview text={preview.text} delimiter={preview.delimiter} />
        {:else}
          {#key target.device}
            <DeviceProvider device={target.device}>
              <MessageResponse content={preview.text} markdown sessionId={target.sessionId} linkBase={preview.path} class="h-auto" />
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
  {#if openFile}<footer class="flex shrink-0 justify-around border-t p-2">{@render actions()}</footer>{/if}
</div>

<style>
  .file-source :global(pre.shiki) {
    min-width: 100%;
    width: max-content;
    margin: 0;
    background: transparent !important;
  }
</style>
