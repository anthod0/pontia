<script lang="ts">
  import { currentLocale, t } from '$lib/i18n'
  import { formatDateTime, formatRelativeTime } from '$lib/format'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import GitCommitIcon from 'phosphor-svelte/lib/GitCommitIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitBranch } from '../../../api/types'
  import { moveRowFocus, shortSha } from './gitChanges'
  import type { GitPanelState } from './gitPanelState.svelte'

  /** A branch picker over the commit history of the chosen branch. Choosing a branch only reads its history. */
  let { device, workspaceId, git }: { device: DeviceRuntime; workspaceId: string; git: GitPanelState } = $props()

  // The panel recreates this view for each device and Workspace.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const branchesQuery = queries.createWorkspaceGitBranchesQuery(() => workspaceId)
  const branches = $derived(branchesQuery.data?.branches ?? [])
  const head = $derived(branchesQuery.data?.head ?? null)
  // A chosen branch that no longer exists falls back to the checked-out one.
  const ref = $derived(git.ref && branches.some(branch => branch.ref === git.ref) ? git.ref : head)
  const commitsQuery = queries.createWorkspaceGitCommitsQuery(() => workspaceId, () => ref)
  const commits = $derived(commitsQuery.data?.pages.flatMap(page => page.commits) ?? [])
  const local = $derived(branches.filter(branch => !branch.remote))
  const remote = $derived(branches.filter(branch => branch.remote))
  const selected = $derived(branches.find(branch => branch.ref === ref))
  let list = $state<HTMLElement>()

  export function fetching(): boolean {
    return branchesQuery.isFetching || commitsQuery.isFetching
  }

  export function refresh() {
    void branchesQuery.refetch()
    if (ref) void commitsQuery.refetch()
  }
</script>

{#snippet branchItem(branch: GitBranch)}
  <Select.Item value={branch.ref} label={branch.name} class="py-1.5">
    <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{branch.name}</span>
    {#if branch.current}<Badge variant="outline" class="ml-2 shrink-0 font-normal">{$t('common.current')}</Badge>{/if}
  </Select.Item>
{/snippet}

{#if branchesQuery.isError}
  <Alert.Root variant="destructive" class="m-3 w-auto">
    <WarningCircleIcon />
    <Alert.Title>{$t('git.branchesFailed')}</Alert.Title>
    <Alert.Description>
      <span class="font-mono text-xs break-words text-foreground">{branchesQuery.error.message}</span>
      <Button variant="outline" size="sm" class="mt-2" onclick={refresh}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
    </Alert.Description>
  </Alert.Root>
{:else if !branchesQuery.data}
  <div role="status" aria-label={$t('common.loading')} class="space-y-3 p-4">
    {#each [70, 52, 64, 45, 58] as width, index (index)}
      <div class="space-y-1.5"><Skeleton class="h-3.5" style="width: {width}%" /><Skeleton class="h-3 w-[40%]" /></div>
    {/each}
  </div>
{:else if !ref}
  <Empty.Root class="min-h-0 p-8">
    <Empty.Header>
      <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><GitCommitIcon class="size-6" /></Empty.Media>
      <Empty.Title>{$t('git.noCommits')}</Empty.Title>
      <Empty.Description>{$t('git.noCommitsDescription')}</Empty.Description>
    </Empty.Header>
  </Empty.Root>
{:else}
  <div class="flex shrink-0 items-center gap-2 border-b px-3 py-2">
    <Select.Root type="single" value={ref} onValueChange={(next) => git.chooseRef(next)}>
      <Select.Trigger class="h-8 min-w-0 flex-1 max-lg:h-10" aria-label={$t('git.branchToShow', { branch: selected?.name ?? $t('git.detachedHead') })}
        title={$t('git.branchPickerHint')}>
        <GitBranchIcon class="size-4 text-muted-foreground" />
        <span class="min-w-0 flex-1 truncate text-left font-mono text-[13px]">{selected?.name ?? $t('git.detachedHead')}</span>
        {#if selected?.current}<span class="shrink-0 text-xs text-muted-foreground">{$t('common.current')}</span>{/if}
      </Select.Trigger>
      <Select.Content align="start" class="max-h-80 w-72">
        {#if head === 'HEAD'}
          <Select.Item value="HEAD" label={$t('git.detachedHead')} class="py-1.5">
            <span class="min-w-0 flex-1 truncate">{$t('git.detachedHead')}</span>
            <Badge variant="outline" class="ml-2 shrink-0 font-normal">{$t('common.current')}</Badge>
          </Select.Item>
        {/if}
        {#if local.length}
          <Select.Group>
            <Select.GroupHeading>{$t('git.localBranches')}</Select.GroupHeading>
            {#each local as branch (branch.ref)}{@render branchItem(branch)}{/each}
          </Select.Group>
        {/if}
        {#if remote.length}
          <Select.Group>
            <Select.GroupHeading>{$t('git.remoteBranches')}</Select.GroupHeading>
            {#each remote as branch (branch.ref)}{@render branchItem(branch)}{/each}
          </Select.Group>
        {/if}
      </Select.Content>
    </Select.Root>
  </div>

  <div class="min-h-0 flex-1 overflow-auto overscroll-contain">
    {#if commitsQuery.isError && !commits.length}
      <Alert.Root variant="destructive" class="m-3 w-auto">
        <WarningCircleIcon />
        <Alert.Title>{$t('git.historyFailed')}</Alert.Title>
        <Alert.Description>
          <span class="font-mono text-xs break-words text-foreground">{commitsQuery.error.message}</span>
          <Button variant="outline" size="sm" class="mt-2" onclick={refresh}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
        </Alert.Description>
      </Alert.Root>
    {:else if !commitsQuery.data}
      <div role="status" aria-label={$t('common.loading')} class="space-y-3 p-4">
        {#each [70, 52, 64, 45, 58] as width, index (index)}
          <div class="space-y-1.5"><Skeleton class="h-3.5" style="width: {width}%" /><Skeleton class="h-3 w-[40%]" /></div>
        {/each}
      </div>
    {:else}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul bind:this={list} class="px-2 py-1" aria-label={$t('git.commits')} onkeydown={(event) => moveRowFocus(event, list!)}>
        {#each commits as commit (commit.sha)}
          <li>
            <button type="button" data-git-row={commit.sha} onclick={() => git.openCommit(commit.sha)}
              class="flex w-full min-w-0 flex-col gap-0.5 rounded-md px-2.5 py-1.5 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring max-lg:py-2.5">
              <span class="w-full truncate text-[13px] text-foreground">{commit.subject || $t('git.noSubject')}</span>
              <span class="flex w-full min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                <span class="shrink-0 font-mono text-primary">{shortSha(commit.sha)}</span>
                <span class="min-w-0 truncate">{commit.author_name}</span>
                {#if commit.parents.length > 1}<span class="shrink-0">· {$t('git.merge')}</span>{/if}
                <time class="ml-auto shrink-0 tabular-nums" datetime={commit.authored_at} title={formatDateTime(commit.authored_at, $currentLocale)}>{formatRelativeTime(commit.authored_at, $currentLocale)}</time>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if commitsQuery.hasNextPage}
        <div class="px-4 pb-4 text-center">
          <Button variant="outline" size="sm" disabled={commitsQuery.isFetchingNextPage} onclick={() => commitsQuery.fetchNextPage()}>{$t('common.loadMore')}</Button>
        </div>
      {/if}
    {/if}
  </div>
{/if}
