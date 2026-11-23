<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import ClockIcon from 'phosphor-svelte/lib/ClockIcon'
  import CloudArrowUpIcon from 'phosphor-svelte/lib/CloudArrowUpIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangesView } from '../../../api/types'
  import GitChangeList from './GitChangeList.svelte'
  import GitCommitDetail from './GitCommitDetail.svelte'
  import GitCommitDiffView from './GitCommitDiffView.svelte'
  import GitDiffView from './GitDiffView.svelte'
  import GitHistory from './GitHistory.svelte'
  import { changeKey, focusRow, sameChange } from './gitChanges'
  import type { GitPanelState, GitView } from './gitPanelState.svelte'

  /** The header above the lists, given the loaded changes and a refresh action. */
  type Header = Snippet<[{ changes: GitChangesView | undefined; refresh: () => void; fetching: boolean }]>

  /**
   * One Workspace's Git tab: its uncommitted changes or a branch's commit history, and the diff,
   * commit or commit file opened from them. It only reads the repository.
   */
  let { device, workspaceId, git, hint, header }: {
    device: DeviceRuntime
    workspaceId: string
    git: GitPanelState
    /** The keyboard hint in the footer. */
    hint: string
    header: Header
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

  // The header summary comes from the status projection, which the page keeps fresh.
  let now = $state(Date.now())
  const clock = setInterval(() => { now = Date.now() }, 1000)
  onDestroy(() => clearInterval(clock))
  const updatedSeconds = $derived(changesQuery.dataUpdatedAt ? Math.max(0, Math.round((now - changesQuery.dataUpdatedAt) / 1000)) : null)

  function refresh() {
    void changesQuery.refetch()
    void queries.refreshWorkspaceGitStatus(workspaceId).catch(() => {})
    if (git.view === 'history') history?.refresh()
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

<div bind:this={element} class="flex min-h-0 flex-1 flex-col">
  {#if git.view === 'changes' && git.diff}
    {#key `${workspaceId}\0${changeKey(git.diff)}`}
      <GitDiffView {device} {workspaceId} file={git.diff} {change} onback={closeDiff} />
    {/key}
  {:else if git.view === 'history' && git.commit && git.commitFile}
    {#key `${workspaceId}\0${git.commit}\0${git.commitFile.path}`}
      <GitCommitDiffView {device} {workspaceId} sha={git.commit} file={git.commitFile} onback={closeCommitFile} />
    {/key}
  {:else if git.view === 'history' && git.commit}
    {#key `${workspaceId}\0${git.commit}`}
      <GitCommitDetail {device} {workspaceId} sha={git.commit} onback={closeCommit}
        onopen={(file) => { git.commitFile = { path: file.path, orig_path: file.orig_path } }} />
    {/key}
  {:else}
    {@render header({ changes, refresh, fetching: changesQuery.isFetching || !!history?.fetching() })}

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
      <Tabs.Root bind:value={() => git.view, (next) => { git.view = next as GitView }} class="min-h-0 flex-1 gap-0">
        <div class="shrink-0 border-b px-3 py-2">
          <Tabs.List class="w-full max-lg:h-10" aria-label={$t('git.views')}>
            <Tabs.Trigger value="changes">{$t('git.changesView')}</Tabs.Trigger>
            <Tabs.Trigger value="history">{$t('git.historyView')}</Tabs.Trigger>
          </Tabs.List>
        </div>
        <Tabs.Content value="changes" class="flex min-h-0 flex-col">
          <div class="min-h-0 flex-1 overflow-auto overscroll-contain">
            <div class="border-b px-4 py-3">
              <div class="flex min-w-0 items-center gap-2 text-sm">
                <GitBranchIcon class="size-4 shrink-0 text-muted-foreground" />
                <span class="min-w-0 truncate font-mono text-[13px] font-medium text-heading">{status?.branch ?? $t('git.noBranch')}</span>
                {#if status?.upstream}
                  <span class="flex shrink-0 gap-1.5 font-mono text-xs">
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
              <GitChangeList files={changes.files} onopen={(item) => { git.diff = { path: item.path, orig_path: item.orig_path, group: item.group } }} />
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
          </div>
          <footer class="flex h-9 shrink-0 items-center gap-1.5 border-t px-4 text-xs text-muted-foreground">
            <ClockIcon class="size-3.5" />
            <span>{updatedSeconds === null ? '' : $t('git.updatedSecondsAgo', { count: updatedSeconds })}</span>
            <span class="flex-1"></span>
            <span class="max-lg:hidden">{hint}</span>
          </footer>
        </Tabs.Content>
        <Tabs.Content value="history" class="flex min-h-0 flex-col">
          <GitHistory bind:this={history} {device} {workspaceId} {git} />
        </Tabs.Content>
      </Tabs.Root>
    {/if}
  {/if}
</div>
