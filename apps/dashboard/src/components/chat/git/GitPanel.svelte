<script lang="ts">
  import { onDestroy, onMount, tick, type Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangedFile, GitCommitFile } from '../../../api/types'
  import GitBranchPicker from './GitBranchPicker.svelte'
  import GitChangeList from './GitChangeList.svelte'
  import GitCommitDetail from './GitCommitDetail.svelte'
  import GitCommitDiffView from './GitCommitDiffView.svelte'
  import GitDiffView from './GitDiffView.svelte'
  import GitHistory from './GitHistory.svelte'
  import GitSection from './GitSection.svelte'
  import { changeKey, focusRow, groupChanges, moveRowFocus, sameChange } from './gitChanges'
  import type { GitPanelState } from './gitPanelState.svelte'

  let { device, workspaceId, git, workspaceHeading, onopen, oncommitfile, onstatuserror }: {
    device: DeviceRuntime
    workspaceId: string
    git: GitPanelState
    workspaceHeading?: Snippet
    onopen?: (file: GitChangedFile, files: GitChangedFile[]) => void
    oncommitfile?: (sha: string, file: GitCommitFile, files: GitCommitFile[]) => void
    onstatuserror?: (error: Error) => void
  } = $props()

  // The parent recreates this view for each device and Workspace.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const changesQuery = queries.createWorkspaceGitChangesQuery(() => workspaceId, () => true)
  const statuses = queries.createWorkspaceGitStatusesStore()
  const status = $derived($statuses[workspaceId])
  const changes = $derived(changesQuery.data)
  const orderedFiles = $derived(changes ? groupChanges(changes.files).flatMap(group => group.files) : [])
  const change = $derived(git.diff ? changes?.files.find(item => sameChange(item, git.diff!)) : undefined)
  const outsideRepository = $derived(changes?.repo_root === null)
  const canReadRepository = $derived(!outsideRepository && (!!changes || changesQuery.isError))
  const branchesQuery = queries.createWorkspaceGitBranchesQuery(() => workspaceId, () => canReadRepository && git.historyOpen)
  const branches = $derived(branchesQuery.data?.branches ?? [])
  const head = $derived(branchesQuery.data?.head ?? null)
  const ref = $derived(git.ref && branches.some(branch => branch.ref === git.ref) ? git.ref : head)
  let element = $state<HTMLElement>()
  let scrollRoot = $state<HTMLElement>()
  let history = $state<GitHistory>()
  let panelWidth = $state(0)
  let now = $state(Date.now())
  let sectionsLoaded = $state(false)
  let refreshing = $state(false)
  let selectedCommitFiles = $state.raw<GitCommitFile[]>([])
  let refreshController: AbortController | undefined
  let previousScroll = 0
  const wide = $derived(panelWidth >= 720)
  const minutes = $derived(Math.max(0, Math.floor((now - changesQuery.dataUpdatedAt) / 60_000)))
  const branchName = $derived(status?.branch ?? $t('git.detachedHead'))
  const branchTooltip = $derived([
    status?.upstream ? `${branchName} → ${status.upstream}` : branchName,
    status?.ahead ? $t('git.ahead', { count: status.ahead }) : '',
    status?.behind ? $t('git.behind', { count: status.behind }) : '',
    changesQuery.dataUpdatedAt ? $t('git.updatedMinutesAgo', { count: minutes }) : '',
  ].filter(Boolean).join(' · '))

  onMount(() => {
    try {
      const saved = localStorage.getItem(`pontia.git.sections.${workspaceId}`)
      if (saved) {
        const value = JSON.parse(saved) as { changes?: boolean; history?: boolean }
        if (typeof value.changes === 'boolean') git.changesOpen = value.changes
        if (typeof value.history === 'boolean') git.historyOpen = value.history
      }
    } catch (error) {
      if (!(error instanceof SyntaxError)) throw error
    } finally {
      sectionsLoaded = true
    }
    const timer = setInterval(() => { now = Date.now() }, 60_000)
    return () => clearInterval(timer)
  })

  onDestroy(() => refreshController?.abort())

  $effect(() => {
    if (sectionsLoaded) localStorage.setItem(`pontia.git.sections.${workspaceId}`, JSON.stringify({ changes: git.changesOpen, history: git.historyOpen }))
  })

  $effect(() => {
    if (wide && changes?.files.length && !git.diff && !git.commit) selectChange(orderedFiles[0]!)
  })

  async function refresh() {
    if (refreshing) return
    refreshing = true
    const controller = new AbortController()
    refreshController = controller
    const requests: Promise<unknown>[] = [
      changesQuery.refetch(),
      queries.refreshWorkspaceGitStatus(workspaceId, { signal: controller.signal }).catch(error => {
        if (!controller.signal.aborted) onstatuserror?.(error)
        throw error
      }),
    ]
    if (canReadRepository && git.historyOpen) {
      requests.push(branchesQuery.refetch())
      const historyRefresh = history?.refresh()
      if (historyRefresh) requests.push(historyRefresh)
    }
    await Promise.allSettled(requests)
    if (refreshController === controller) {
      refreshController = undefined
      refreshing = false
    }
  }

  export function keydown(event: KeyboardEvent) {
    if ((event.key === '[' || event.key === ']') && (git.diff || git.commitFile)) {
      const files = git.diff ? orderedFiles : selectedCommitFiles
      const index = files.findIndex(file => file.path === (git.diff?.path ?? git.commitFile?.path))
      const next = files[index + (event.key === ']' ? 1 : -1)]
      if (!next) return
      event.preventDefault()
      if (git.diff) selectChange(next as GitChangedFile)
      else git.commitFile = { path: next.path, orig_path: next.orig_path }
      return
    }
    if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && scrollRoot && (!git.diff || wide) && (!git.commitFile || wide)) {
      moveRowFocus(event, scrollRoot, wide)
      return
    }
    if (event.key !== 'Escape' || event.defaultPrevented) return
    if (git.commitFile) closeCommitFile()
    else if (!wide && git.diff && !onopen) closeDiff()
  }

  function selectChange(item: GitChangedFile) {
    git.diff = { path: item.path, orig_path: item.orig_path, group: item.group }
    git.commit = null
    git.commitFile = null
  }

  function openChange(item: GitChangedFile) {
    if (!wide && onopen) return onopen(item, orderedFiles)
    previousScroll = scrollRoot?.scrollTop ?? 0
    if (!wide) element?.focus()
    selectChange(item)
  }

  function openCommit(sha: string) {
    git.diff = null
    git.commitFile = null
    git.commit = !wide && git.commit === sha ? null : sha
  }

  function openCommitFile(sha: string, file: GitCommitFile, files: GitCommitFile[]) {
    if (!wide && oncommitfile) return oncommitfile(sha, file, files)
    previousScroll = scrollRoot?.scrollTop ?? 0
    git.commit = sha
    selectedCommitFiles = files
    git.commitFile = { path: file.path, orig_path: file.orig_path }
  }

  async function restoreScroll() {
    await tick()
    if (scrollRoot) scrollRoot.scrollTop = previousScroll
  }

  function closeDiff() {
    const previous = git.diff!
    git.diff = null
    void restoreScroll().then(() => focusRow(element, changeKey(previous)))
  }

  function closeCommitFile() {
    const previous = git.commitFile!
    git.commitFile = null
    void restoreScroll().then(() => focusRow(element, `${git.commit}:${previous.path}`))
  }
</script>

{#snippet notRepository()}
  <Empty.Root class="min-h-0 p-8"><Empty.Header><Empty.Media class="size-12 rounded-item bg-muted text-muted-foreground"><FolderIcon class="size-6" /></Empty.Media><Empty.Title>{$t('git.notRepository')}</Empty.Title><Empty.Description>{$t('git.notRepositoryDescription')}</Empty.Description></Empty.Header></Empty.Root>
{/snippet}

{#snippet toolbar()}
  <header class="flex h-11 shrink-0 items-center gap-1 border-b pr-2.5 pl-2 max-lg:h-[52px]">
    {#if workspaceHeading}
      <div class="flex min-w-0 items-center">{@render workspaceHeading()}</div>
      <span class="mx-1 h-4 w-px shrink-0 bg-border"></span>
    {/if}
    {#if status?.repo_root}
      <div class="flex h-7 min-w-0 items-center gap-1.5 rounded-chip px-2" title={branchTooltip}>
        <GitBranchIcon class="size-3.5 shrink-0 text-muted-foreground" />
        <span class="truncate font-mono text-xs font-medium text-foreground">{branchName}</span>
        {#if status.ahead}<span class="shrink-0 font-mono text-xs text-primary">↑{status.ahead}</span>{/if}
        {#if status.behind}<span class="shrink-0 font-mono text-xs text-interrupted">↓{status.behind}</span>{/if}
      </div>
      {#if status.upstream}<span class={['upstream min-w-0 truncate font-mono text-11 text-muted-foreground', workspaceHeading && 'bot-upstream']}>→ {status.upstream}</span>{/if}
    {/if}
    <span class="min-w-0 flex-1"></span>
    {#if changesQuery.dataUpdatedAt && minutes > 5}<time class="shrink-0 text-11 text-muted-foreground tabular-nums" datetime={new Date(changesQuery.dataUpdatedAt).toISOString()}>{$t('git.minutesAgo', { count: minutes })}</time>{/if}
    <Button variant="ghost" size="icon-circle-sm" class="-mr-1.5 size-7 p-0 max-lg:size-10" disabled={refreshing} onclick={refresh} aria-label={$t('common.refresh')} title={`${$t('common.refresh')} · ${$t('git.updatedMinutesAgo', { count: minutes })}`}><ArrowClockwiseIcon class={['size-4', refreshing && 'animate-spin']} /></Button>
  </header>
{/snippet}

{#snippet sections()}
  <div bind:this={scrollRoot} class="min-h-0 flex-1 overflow-auto overscroll-contain pb-3 max-lg:pb-6">
    <GitSection kind="changes" title={$t('git.changesView')} count={changes?.files.length ? changes.total : undefined}
      description={changes && !changes.files.length ? $t('git.noChanges') : undefined} open={git.changesOpen}
      disabled={!!changes && !changes.files.length} {scrollRoot} onchange={(open) => { git.changesOpen = open }}>
      {#snippet children()}
        {#if changesQuery.isError && !changes}
          <Alert.Root variant="destructive" class="m-3 w-auto"><WarningCircleIcon /><Alert.Title>{$t('git.readFailed')}</Alert.Title><Alert.Description><span class="font-mono text-xs break-words text-foreground">{changesQuery.error.message}</span><Button variant="outline" size="sm" class="mt-2" onclick={refresh}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button></Alert.Description></Alert.Root>
        {:else if !changes}
          <div role="status" aria-label={$t('common.loading')} class="space-y-2 p-3">{#each [60, 80, 52, 74] as width}<div class="flex h-[30px] items-center gap-2.5 px-2"><Skeleton class="h-3 w-3" /><Skeleton class="h-3" style="width: {width}%" /></div>{/each}</div>
        {:else if changes.files.length}
          <GitChangeList files={changes.files} selected={wide ? git.diff : null} onopen={openChange} />
          {#if changes.truncated}<p class="px-4 pb-4 text-center text-xs text-muted-foreground">{$t('git.listTruncated', { shown: changes.files.length, total: changes.total })}</p>{/if}
        {/if}
      {/snippet}
    </GitSection>

    <GitSection kind="history" title={$t('git.historyView')} open={git.historyOpen} {scrollRoot} onchange={(open) => { git.historyOpen = open }}>
      {#snippet actions()}
        {#if branchesQuery.data && (branches.length || ref)}<GitBranchPicker {branches} {head} {ref} onchoose={(next) => git.chooseRef(next)} />{/if}
      {/snippet}
      {#snippet children()}
        {#if branchesQuery.isError}
          <Alert.Root variant="destructive" class="m-3 w-auto"><WarningCircleIcon /><Alert.Title>{$t('git.branchesFailed')}</Alert.Title><Alert.Description>{branchesQuery.error.message}<Button variant="outline" size="sm" onclick={refresh}>{$t('common.retry')}</Button></Alert.Description></Alert.Root>
        {:else if !branchesQuery.data}
          <div role="status" aria-label={$t('common.loading')} class="p-4"><Skeleton class="h-4 w-2/3" /></div>
        {:else}
          <GitHistory bind:this={history} {device} {workspaceId} {ref} upstream={status?.upstream} selected={git.commit}
            inline={!wide} selectFirst={wide && !orderedFiles.length} oncommit={openCommit} onfile={openCommitFile} />
        {/if}
      {/snippet}
    </GitSection>
  </div>
{/snippet}

<div bind:this={element} bind:clientWidth={panelWidth} tabindex="-1" class="git-panel flex h-full min-h-0 flex-1 flex-col outline-none">
  {#if !wide && canReadRepository && git.diff && !onopen}
    <GitDiffView {device} {workspaceId} file={git.diff} files={orderedFiles} {change} onback={closeDiff} onselect={selectChange} />
  {:else if !wide && canReadRepository && git.commit && git.commitFile && !oncommitfile}
    <GitCommitDiffView {device} {workspaceId} sha={git.commit} file={git.commitFile} onback={closeCommitFile} onselect={(file) => { git.commitFile = file }} />
  {:else}
    {@render toolbar()}
    {#if outsideRepository}
      {@render notRepository()}
    {:else if wide}
      <div class="grid min-h-0 flex-1 grid-cols-[360px_minmax(0,1fr)]">
        <div class="flex min-h-0 flex-col border-r">{@render sections()}</div>
        <div class="flex min-h-0 min-w-0 flex-col">
          {#if git.diff}
            <GitDiffView {device} {workspaceId} file={git.diff} files={orderedFiles} {change} onselect={selectChange} />
          {:else if git.commit && git.commitFile}
            <GitCommitDiffView {device} {workspaceId} sha={git.commit} file={git.commitFile} onback={closeCommitFile} onselect={(file) => { git.commitFile = file }} />
          {:else if git.commit}
            <GitCommitDetail {device} {workspaceId} sha={git.commit} onopen={(file, files) => openCommitFile(git.commit!, file, files)} onparent={openCommit} />
          {/if}
        </div>
      </div>
    {:else}
      {@render sections()}
    {/if}
  {/if}
</div>

<style>
  .git-panel { container-type: inline-size; }
  @container (width < 400px) { .upstream { display: none; } }
  .bot-upstream { display: none; }
</style>
