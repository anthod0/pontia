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
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitChangesView } from '../../../api/types'
  import type { GitDiffTarget } from '../../../queries/workspaces'
  import GitChangeList from './GitChangeList.svelte'
  import GitDiffView from './GitDiffView.svelte'
  import { sameChange } from './gitChanges'

  /** The header above the change list, given the loaded changes and a refresh action. */
  type Header = Snippet<[{ changes: GitChangesView | undefined; refresh: () => void; fetching: boolean }]>

  /** One Workspace's uncommitted changes: the change list, or the diff of `file` when one is open. */
  let { device, workspaceId, file, hint, header, onopen, onback, onclose }: {
    device: DeviceRuntime
    workspaceId: string
    file: GitDiffTarget | null
    /** The keyboard hint in the footer. */
    hint: string
    header: Header
    onopen: (file: GitDiffTarget) => void
    onback: () => void
    /** Closes the surrounding panel; without it, the diff has no close button and Esc stops at the list. */
    onclose?: () => void
  } = $props()

  // The parent recreates this view for each device and Workspace.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const changesQuery = queries.createWorkspaceGitChangesQuery(() => workspaceId, () => true)
  const statuses = queries.createWorkspaceGitStatusesStore()
  const status = $derived($statuses[workspaceId])
  const changes = $derived(changesQuery.data)
  const change = $derived(file ? changes?.files.find(item => sameChange(item, file)) : undefined)
  let element = $state<HTMLElement>()

  // The header summary comes from the status projection, which the page keeps fresh.
  let now = $state(Date.now())
  const clock = setInterval(() => { now = Date.now() }, 1000)
  onDestroy(() => clearInterval(clock))
  const updatedSeconds = $derived(changesQuery.dataUpdatedAt ? Math.max(0, Math.round((now - changesQuery.dataUpdatedAt) / 1000)) : null)

  function refresh() {
    void changesQuery.refetch()
    void queries.refreshWorkspaceGitStatus(workspaceId).catch(() => {})
  }

  /** Esc returns from a diff to the list, then closes the panel. The parent forwards keys from the element that holds focus. */
  export function keydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.defaultPrevented) return
    if (file) back()
    else if (onclose) onclose()
    else return
    event.preventDefault()
  }

  function back() {
    const previous = file
    onback()
    // Return focus to the row the diff was opened from.
    queueMicrotask(() => {
      if (!previous) return
      const row = [...element?.querySelectorAll<HTMLElement>('[data-git-change-row]') ?? []]
        .find(row => row.dataset.path === previous.path && row.dataset.group === previous.group)
      row?.focus()
    })
  }
</script>

<div bind:this={element} class="flex min-h-0 flex-1 flex-col">
  {#if file}
    {#key `${workspaceId}\0${file.group}\0${file.path}`}
      <GitDiffView {device} {workspaceId} {file} {change} onback={back} {onclose} />
    {/key}
  {:else}
    {@render header({ changes, refresh, fetching: changesQuery.isFetching })}

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
          <GitChangeList files={changes.files} onopen={(item) => onopen({ path: item.path, orig_path: item.orig_path, group: item.group })} />
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
        <span class="max-lg:hidden">{hint}</span>
      </footer>
    {/if}
  {/if}
</div>
