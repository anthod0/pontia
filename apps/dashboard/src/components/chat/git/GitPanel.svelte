<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import { number, t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangedFile } from '../../../api/types'
  import GitChangeList from './GitChangeList.svelte'
  import GitCommitDetail from './GitCommitDetail.svelte'
  import GitCommitDiffView from './GitCommitDiffView.svelte'
  import GitDiffView from './GitDiffView.svelte'
  import GitBranchPicker from './GitBranchPicker.svelte'
  import GitHistory from './GitHistory.svelte'
  import { changeKey, focusRow, sameChange } from './gitChanges'
  import type { GitPanelState, GitView } from './gitPanelState.svelte'

  let { device, workspaceId, git, hint, workspaceHeading, onopen, oncommit, onstatuserror }: {
    device: DeviceRuntime
    workspaceId: string
    git: GitPanelState
    hint?: string
    workspaceHeading?: Snippet
    onopen?: (file: GitChangedFile) => void
    oncommit?: (sha: string) => void
    onstatuserror?: (error: Error) => void
  } = $props()

  // The parent recreates this view for each device and Workspace.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const changesQuery = queries.createWorkspaceGitChangesQuery(() => workspaceId, () => true)
  const statuses = queries.createWorkspaceGitStatusesStore()
  const status = $derived($statuses[workspaceId])
  const changes = $derived(changesQuery.data)
  const change = $derived(git.diff ? changes?.files.find(item => sameChange(item, git.diff!)) : undefined)
  let element = $state<HTMLElement>()
  let history = $state<GitHistory>()

  const outsideRepository = $derived(changes?.repo_root === null)
  const canReadRepository = $derived(!outsideRepository && (!!changes || changesQuery.isError))
  const branchesQuery = queries.createWorkspaceGitBranchesQuery(() => workspaceId, () => canReadRepository && git.view === 'history' && (!git.commit || !!oncommit))
  const branches = $derived(branchesQuery.data?.branches ?? [])
  const head = $derived(branchesQuery.data?.head ?? null)
  const ref = $derived(git.ref && branches.some(branch => branch.ref === git.ref) ? git.ref : head)
  let now = $state(Date.now())
  onMount(() => {
    const timer = setInterval(() => { now = Date.now() }, 60_000)
    return () => clearInterval(timer)
  })
  const minutes = $derived(Math.max(0, Math.floor((now - changesQuery.dataUpdatedAt) / 60_000)))
  const fetching = $derived(changesQuery.isFetching || (git.view === 'history' && (branchesQuery.isFetching || !!history?.fetching())))
  const segment = 'rounded-full px-3 text-xs aria-selected:bg-card dark:aria-selected:bg-[#45403d] group-data-[variant=default]/tabs-list:aria-selected:shadow-sm'

  function refresh() {
    void changesQuery.refetch()
    void queries.refreshWorkspaceGitStatus(workspaceId).catch(error => onstatuserror?.(error))
    if (canReadRepository && git.view === 'history') {
      void branchesQuery.refetch()
      history?.refresh()
    }
  }

  /** Esc steps back one level: from a diff to its list, or from a commit to the history. The parent forwards keys from the element that holds focus. */
  export function keydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.defaultPrevented) return
    if (!git.diff && !git.commit) return
    event.preventDefault()
    if (git.commitFile) closeCommitFile()
    else if (git.commit) closeCommit()
    else closeDiff()
  }

  function openChange(item: GitChangedFile) {
    if (onopen) return onopen(item)
    element?.focus()
    git.diff = { path: item.path, orig_path: item.orig_path, group: item.group }
  }

  function openCommit(sha: string) {
    if (oncommit) return oncommit(sha)
    element?.focus()
    git.openCommit(sha)
  }

  // Each step back returns focus to the row it was opened from.
  function closeDiff() {
    const previous = git.diff!
    git.diff = null
    focusRow(element, changeKey(previous))
  }

  function closeCommit() {
    const previous = git.commit!
    git.commit = null
    focusRow(element, previous)
  }

  function closeCommitFile() {
    const previous = git.commitFile!
    git.commitFile = null
    focusRow(element, previous.path)
  }
</script>

{#snippet notRepository()}
  <Empty.Root class="min-h-0 p-8">
    <Empty.Header>
      <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FolderIcon class="size-6" /></Empty.Media>
      <Empty.Title>{$t('git.notRepository')}</Empty.Title>
      <Empty.Description>{$t('git.notRepositoryDescription')}</Empty.Description>
    </Empty.Header>
  </Empty.Root>
{/snippet}

<div bind:this={element} tabindex="-1" class="flex h-full min-h-0 flex-1 flex-col outline-none">
  {#if canReadRepository && git.view === 'changes' && git.diff && !onopen}
    {#key `${workspaceId}\0${changeKey(git.diff)}`}
      <GitDiffView {device} {workspaceId} file={git.diff} {change} onback={closeDiff} />
    {/key}
  {:else if canReadRepository && git.view === 'history' && git.commit && git.commitFile && !oncommit}
    {#key `${workspaceId}\0${git.commit}\0${git.commitFile.path}`}
      <GitCommitDiffView {device} {workspaceId} sha={git.commit} file={git.commitFile} onback={closeCommitFile} />
    {/key}
  {:else if canReadRepository && git.view === 'history' && git.commit && !oncommit}
    {#key `${workspaceId}\0${git.commit}`}
      <GitCommitDetail {device} {workspaceId} sha={git.commit} onback={closeCommit}
        onopen={(file) => { element?.focus(); git.commitFile = { path: file.path, orig_path: file.orig_path } }} />
    {/key}
  {:else}
    <Tabs.Root bind:value={() => git.view, (next) => { git.view = next as GitView }} class="min-h-0 flex-1 gap-0">
      <header class="grid shrink-0 grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-x-2 border-b py-2 pr-2 pl-4 max-lg:grid-cols-[minmax(0,1fr)_auto] max-lg:gap-y-1 max-lg:px-2">
        {#if workspaceHeading}<div class="flex min-w-0 items-center gap-2 max-lg:col-span-2">{@render workspaceHeading()}</div>{/if}
        <Tabs.List aria-label={$t('git.views')} class="h-[30px] shrink-0 rounded-full p-0.5 group-data-horizontal/tabs:h-[30px] max-lg:order-last max-lg:col-span-2 max-lg:h-10 max-lg:w-full max-lg:group-data-horizontal/tabs:h-10">
          <Tabs.Trigger value="changes" class={segment}>{$t('git.changesView')}{#if changes?.repo_root}<span class="text-muted-foreground tabular-nums" title={$t('git.changeCount', { count: changes.total })}>{$number(changes.total)}</span>{/if}</Tabs.Trigger>
          <Tabs.Trigger value="history" class={segment}>{$t('git.historyView')}</Tabs.Trigger>
        </Tabs.List>
        <Button variant="ghost" size="icon-sm" class="rounded-full p-0 size-8 max-lg:order-2 max-lg:size-11" disabled={fetching} onclick={refresh} aria-label={$t('common.refresh')} title={$t('common.refresh')}>
          <ArrowClockwiseIcon class={['size-4', fetching && 'animate-spin']} />
        </Button>
        <div class="col-span-3 flex h-6 min-w-0 items-center gap-1.5 text-xs text-muted-foreground max-lg:order-1 max-lg:col-span-1 max-lg:h-11">
          {#if !outsideRepository && git.view === 'history' && branchesQuery.data && (branches.length || ref)}
            <GitBranchPicker {branches} {head} {ref} onchoose={(next) => git.chooseRef(next)} />
          {:else if status?.repo_root}
            <GitBranchIcon class="size-3.5 shrink-0" />
            <span class="truncate font-mono font-medium text-foreground">{status.branch ?? $t('git.detachedHead')}</span>
            {#if status.ahead}<span class="shrink-0 font-mono text-primary" title={$t('git.ahead', { count: status.ahead })}>↑{status.ahead}</span>{/if}
            {#if status.behind}<span class="shrink-0 font-mono text-interrupted" title={$t('git.behind', { count: status.behind })}>↓{status.behind}</span>{/if}
            <span aria-hidden="true">·</span><span class="min-w-0 truncate font-mono">{status.upstream ?? $t('git.noUpstream')}</span>
          {/if}
          {#if changesQuery.dataUpdatedAt}<time class="ml-auto shrink-0 tabular-nums" datetime={new Date(changesQuery.dataUpdatedAt).toISOString()}>{$t('git.updatedMinutesAgo', { count: minutes })}</time>{/if}
        </div>
      </header>
      <Tabs.Content value="changes" class="flex min-h-0 flex-col">
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
          {#if git.view === 'changes'}{@render notRepository()}{/if}
        {:else}
          <div class="min-h-0 flex-1 overflow-auto overscroll-contain">
            {#if changes.files.length}
              <GitChangeList files={changes.files} onopen={openChange} />
              {#if changes.truncated}
                <p class="px-4 pb-4 text-center text-xs text-muted-foreground">{$t('git.listTruncated', { shown: changes.files.length, total: changes.total })}</p>
              {/if}
              {#if hint}<p class="px-4 py-2 text-xs text-muted-foreground max-lg:hidden">{hint}</p>{/if}
            {:else}
              <Empty.Root class="min-h-0 p-8">
                <Empty.Header>
                  <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><CheckIcon class="size-6" /></Empty.Media>
                  <Empty.Title>{$t('git.noChanges')}</Empty.Title>
                  <Empty.Description>{$t('git.noChangesDescription')}</Empty.Description>
                </Empty.Header>
              </Empty.Root>
            {/if}
          </div>
        {/if}
      </Tabs.Content>
      <Tabs.Content value="history" class="flex min-h-0 flex-col">
        <!-- History is read only while it shows. -->
        {#if git.view === 'history'}
          {#if outsideRepository}
            {@render notRepository()}
          {:else if branchesQuery.isError}
            <Alert.Root variant="destructive" class="m-3 w-auto"><WarningCircleIcon /><Alert.Title>{$t('git.branchesFailed')}</Alert.Title>
              <Alert.Description>{branchesQuery.error.message}<Button variant="outline" size="sm" onclick={refresh}>{$t('common.retry')}</Button></Alert.Description>
            </Alert.Root>
          {:else if !branchesQuery.data}
            <div role="status" aria-label={$t('common.loading')} class="p-4"><Skeleton class="h-4 w-2/3" /></div>
          {:else}
            <GitHistory bind:this={history} {device} {workspaceId} {ref} oncommit={openCommit} />
          {/if}
        {/if}
      </Tabs.Content>
    </Tabs.Root>
  {/if}
</div>
