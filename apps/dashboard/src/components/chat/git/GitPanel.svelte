<script lang="ts">
  import { t, number } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import FilePreviewResizer from '../FilePreviewResizer.svelte'
  import GitChanges from './GitChanges.svelte'
  import type { GitPanel, GitPanelSource } from './gitPanelContext.svelte'

  let { panel, source }: { panel: GitPanel; source: GitPanelSource } = $props()

  let element = $state<HTMLElement>()
  let view = $state<GitChanges>()

  // Opening the panel by hand moves focus into it, so the keyboard works there right away.
  // A remount, such as after switching sessions, leaves focus where it is.
  // svelte-ignore state_referenced_locally
  let handledFocusRequest = panel.focusRequest
  $effect(() => {
    if (panel.focusRequest === handledFocusRequest) return
    handledFocusRequest = panel.focusRequest
    element?.focus()
  })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside bind:this={element} tabindex="-1" data-git-panel aria-label={$t('git.panel')} onkeydown={(event) => view?.keydown(event)}
  class="fixed inset-y-0 right-0 z-50 flex w-full min-w-0 flex-col border-l bg-surface outline-none pt-[env(safe-area-inset-top)] lg:w-[var(--file-preview-width)]">
  <FilePreviewResizer />
  <GitChanges bind:this={view} device={source.device} workspaceId={source.workspaceId} file={panel.file} hint={$t('git.keyboardHint')}
    onopen={(file) => panel.openFile(file)} onback={() => panel.showList()} onclose={() => panel.close()}>
    {#snippet header({ changes, refresh, fetching })}
      <header class="flex h-12 shrink-0 items-center gap-1 border-b px-2 max-lg:h-14">
        <h2 class="flex h-full items-center gap-1.5 border-b-2 border-primary px-2.5 text-sm font-medium text-heading">
          <GitBranchIcon class="size-4" />{$t('git.title')}
          {#if changes?.repo_root}<span class="text-xs font-normal text-muted-foreground tabular-nums" title={$t('git.changeCount', { count: changes.total })}>{$number(changes.total)}</span>{/if}
        </h2>
        <span class="flex-1"></span>
        <Button variant="ghost" size="icon-sm" class="max-lg:size-11" disabled={fetching} onclick={refresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
          <ArrowClockwiseIcon class={['size-4 max-lg:size-5', fetching && 'animate-spin']} />
        </Button>
        <Button variant="ghost" size="icon-sm" class="max-lg:size-11" onclick={() => panel.close()} aria-label={$t('common.close')} title={`${$t('common.close')} (Alt G)`}><XIcon class="size-4 max-lg:size-5" /></Button>
      </header>
    {/snippet}
  </GitChanges>
</aside>
