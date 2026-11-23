<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t, number } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import ClockIcon from 'phosphor-svelte/lib/ClockIcon'
  import CloudArrowUpIcon from 'phosphor-svelte/lib/CloudArrowUpIcon'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import FilePreviewResizer from '../FilePreviewResizer.svelte'
  import GitChangeList from './GitChangeList.svelte'
  import GitDiffView from './GitDiffView.svelte'
  import type { GitPanel, GitPanelSource } from './gitPanelContext.svelte'
  import { sameChange } from './gitChanges'

  let { panel, source }: { panel: GitPanel; source: GitPanelSource } = $props()

  const device = $derived(source.device)
  const workspaceId = $derived(source.workspaceId)
  // svelte-ignore state_referenced_locally
  const queries = source.device.workspaceQueries
  const changesQuery = queries.createWorkspaceGitChangesQuery(() => workspaceId, () => true)
  const statuses = queries.createWorkspaceGitStatusesStore()
  const status = $derived($statuses[workspaceId])
  const changes = $derived(changesQuery.data)
  const file = $derived(panel.file)
  const change = $derived(file ? changes?.files.find(item => sameChange(item, file)) : undefined)
  let element = $state<HTMLElement>()

  // The header summary comes from the status projection, which the session page keeps fresh.
  let now = $state(Date.now())
  const clock = setInterval(() => { now = Date.now() }, 1000)
  onDestroy(() => clearInterval(clock))
  const updatedSeconds = $derived(changesQuery.dataUpdatedAt ? Math.max(0, Math.round((now - changesQuery.dataUpdatedAt) / 1000)) : null)

  function refresh() {
    void changesQuery.refetch()
    void queries.refreshWorkspaceGitStatus(workspaceId).catch(() => {})
  }

  /** Esc returns from a diff to the list, then closes the panel. */
  function keydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.defaultPrevented) return
    event.preventDefault()
    if (panel.file) back()
    else panel.close()
  }

  function back() {
    const previous = panel.file
    panel.showList()
    // Return focus to the row the diff was opened from.
    queueMicrotask(() => {
      if (!previous) return
      const row = [...element?.querySelectorAll<HTMLElement>('[data-git-change-row]') ?? []]
        .find(row => row.dataset.path === previous.path && row.dataset.group === previous.group)
      row?.focus()
    })
  }

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
<aside bind:this={element} tabindex="-1" data-git-panel aria-label={$t('git.panel')} onkeydown={keydown}
  class="fixed inset-y-0 right-0 z-50 flex w-full min-w-0 flex-col border-l bg-surface outline-none pt-[env(safe-area-inset-top)] lg:w-[var(--file-preview-width)]">
  <FilePreviewResizer />
  {#if file}
    {#key `${workspaceId}\0${file.group}\0${file.path}`}
      <GitDiffView {device} {workspaceId} {file} {change} onback={back} onclose={() => panel.close()} />
    {/key}
  {:else}
    <header class="flex h-12 shrink-0 items-center gap-1 border-b px-2 max-lg:h-14">
      <h2 class="flex h-full items-center gap-1.5 border-b-2 border-primary px-2.5 text-sm font-medium text-heading">
        <FileTextIcon class="size-4" />{$t('git.changes')}
        {#if changes?.repo_root}<span class="text-xs font-normal text-muted-foreground tabular-nums">{$number(changes.total)}</span>{/if}
      </h2>
      <span class="flex-1"></span>
      <Button variant="ghost" size="icon-sm" class="max-lg:size-11" disabled={changesQuery.isFetching} onclick={refresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
        <ArrowClockwiseIcon class={['size-4 max-lg:size-5', changesQuery.isFetching && 'animate-spin']} />
      </Button>
      <Button variant="ghost" size="icon-sm" class="max-lg:size-11" onclick={() => panel.close()} aria-label={$t('common.close')} title={`${$t('common.close')} (Alt G)`}><XIcon class="size-4 max-lg:size-5" /></Button>
    </header>

    <div class="min-h-0 flex-1 overflow-auto overscroll-contain">
      {#if changesQuery.isError && !changes}
        <Alert.Root variant="destructive" class="m-3 w-auto">
          <WarningCircleIcon />
          <Alert.Title>{$t('git.readFailed')}</Alert.Title>
          <Alert.Description>
            <span class="font-mono text-xs break-words text-foreground">{changesQuery.error.message}</span>
            <Button variant="outline" size="sm" class="mt-2" onclick={refresh}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
          </Alert.Description>
        </Alert.Root>
      {:else if !changes}
        <div role="status" aria-label={$t('common.loading')}>
          <div class="space-y-1.5 border-b px-4 py-3">
            <Skeleton class="h-3.5 w-[46%]" />
            <Skeleton class="h-3 w-[70%]" />
          </div>
          <div class="p-3">
            {#each [60, 80, 52, 74, 66, 40, 70] as width, index (index)}
              <div class="flex h-[30px] items-center gap-2.5 px-2"><Skeleton class="h-3 w-3" /><Skeleton class="h-3" style="width: {width}%" /></div>
            {/each}
          </div>
        </div>
      {:else if !changes.repo_root}
        <Empty.Root class="min-h-0 p-8">
          <Empty.Header>
            <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FolderIcon class="size-6" /></Empty.Media>
            <Empty.Title>{$t('git.notRepository')}</Empty.Title>
            <Empty.Description>{$t('git.notRepositoryDescription')}</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {:else}
        <div class="border-b px-4 py-3">
          <div class="flex min-w-0 items-center gap-2 text-sm">
            <GitBranchIcon class="size-4 shrink-0 text-muted-foreground" />
            <span class="min-w-0 truncate font-mono font-medium text-heading">{status?.branch ?? $t('git.noBranch')}</span>
            {#if status?.upstream}
              <span class="ml-auto flex shrink-0 gap-1.5 font-mono text-xs">
                {#if status.ahead}<span class="text-primary" title={$t('git.ahead', { count: status.ahead })}>↑{status.ahead}</span>{/if}
                {#if status.behind}<span class="text-interrupted" title={$t('git.behind', { count: status.behind })}>↓{status.behind}</span>{/if}
                {#if !status.ahead && !status.behind}<span class="text-muted-foreground">{$t('git.synced')}</span>{/if}
              </span>
            {/if}
          </div>
          <div class="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
            <CloudArrowUpIcon class="size-3.5 shrink-0" />
            {#if status?.upstream}<span class="min-w-0 truncate font-mono">{status.upstream}</span>
            {:else}<span>{$t('git.noUpstream')}</span>{/if}
          </div>
        </div>
        {#if changes.files.length}
          <GitChangeList files={changes.files} onopen={(item) => panel.openFile({ path: item.path, orig_path: item.orig_path, group: item.group })} />
          {#if changes.truncated}
            <p class="px-4 pb-4 text-center text-xs text-muted-foreground">{$t('git.listTruncated', { shown: changes.files.length, total: changes.total })}</p>
          {/if}
        {:else}
          <Empty.Root class="min-h-0 p-8">
            <Empty.Header>
              <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><CheckIcon class="size-6" /></Empty.Media>
              <Empty.Title>{$t('git.noChanges')}</Empty.Title>
              <Empty.Description>{$t('git.noChangesDescription')}</Empty.Description>
            </Empty.Header>
          </Empty.Root>
        {/if}
      {/if}
    </div>
    {#if changes?.repo_root}
      <footer class="flex h-9 shrink-0 items-center gap-1.5 border-t px-4 text-xs text-muted-foreground">
        <ClockIcon class="size-3.5" />
        <span>{updatedSeconds === null ? '' : $t('git.updatedSecondsAgo', { count: updatedSeconds })}</span>
        <span class="flex-1"></span>
        <span class="max-lg:hidden">{$t('git.keyboardHint')}</span>
      </footer>
    {/if}
  {/if}
</aside>
