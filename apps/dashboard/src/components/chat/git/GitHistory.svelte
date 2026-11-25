<script lang="ts">
  import { currentLocale, t } from '$lib/i18n'
  import { formatDateTime } from '$lib/format'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import GitMergeIcon from 'phosphor-svelte/lib/GitMergeIcon'
  import GitCommitIcon from 'phosphor-svelte/lib/GitCommitIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import { moveRowFocus, shortSha } from './gitChanges'

  let { device, workspaceId, ref, oncommit }: { device: DeviceRuntime; workspaceId: string; ref: string | null; oncommit: (sha: string) => void } = $props()

  // The panel recreates this view for each device and Workspace.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const commitsQuery = queries.createWorkspaceGitCommitsQuery(() => workspaceId, () => ref)
  const commits = $derived(commitsQuery.data?.pages.flatMap(page => page.commits) ?? [])
  const date = $derived(new Intl.DateTimeFormat($currentLocale, { dateStyle: 'medium' }))
  const time = $derived(new Intl.DateTimeFormat($currentLocale, { hour: 'numeric', minute: '2-digit' }))
  const days = $derived.by(() => {
    const groups: { label: string; commits: typeof commits }[] = []
    const today = new Date()
    const yesterday = new Date(today)
    yesterday.setDate(today.getDate() - 1)
    const relativeDay = new Intl.RelativeTimeFormat($currentLocale, { numeric: 'auto' })
    for (const commit of commits) {
      const authored = new Date(commit.authored_at)
      const day = authored.toDateString()
      const label = day === today.toDateString() ? relativeDay.format(0, 'day')
        : day === yesterday.toDateString() ? relativeDay.format(-1, 'day') : date.format(authored)
      if (groups.at(-1)?.label !== label) groups.push({ label, commits: [] })
      groups.at(-1)!.commits.push(commit)
    }
    return groups
  })
  let list = $state<HTMLElement>()

  export function fetching(): boolean {
    return commitsQuery.isFetching
  }

  export function refresh() {
    if (ref) void commitsQuery.refetch()
  }
</script>

<div class="min-h-0 flex-1 overflow-auto overscroll-contain">
  {#if !ref}
    <Empty.Root class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><GitCommitIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.noCommits')}</Empty.Title>
        <Empty.Description>{$t('git.noCommitsDescription')}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else if commitsQuery.isError && !commits.length}
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
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div bind:this={list} class="relative px-2 pb-2 before:absolute before:top-[52px] before:bottom-[30px] before:left-[21px] before:w-px before:bg-border" onkeydown={(event) => moveRowFocus(event, list!)}>
      {#each days as day (day.commits[0].sha)}
        <h3 class="flex h-[30px] items-center pl-8 text-xs font-medium text-muted-foreground">{day.label}</h3>
        <ul aria-label={day.label}>
          {#each day.commits as commit (commit.sha)}
            {@const unpushed = commit.pushed === false}
            {@const status = commit.pushed === undefined ? undefined : $t(unpushed ? 'git.unpushed' : 'git.pushed')}
            <li>
              <button type="button" data-git-row={commit.sha} onclick={() => oncommit(commit.sha)}
                class="group relative flex h-11 w-full min-w-0 items-center gap-2 rounded-lg pr-2 pl-8 text-left outline-none hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset">
                <span aria-hidden="true" class={['absolute left-[9px] size-[9px] rounded-full border border-current', unpushed ? 'text-amber-700 dark:text-amber-400' : 'text-muted-foreground', commit.parents.length > 1 ? 'bg-background' : 'bg-current']}></span>
                <span class="min-w-0 flex-1">
                  <span class="flex items-center gap-1.5 text-[13px]">{#if commit.parents.length > 1}<GitMergeIcon aria-label={$t('git.merge')} class="size-3.5 shrink-0 text-muted-foreground" />{/if}<span class="truncate">{commit.subject || $t('git.noSubject')}</span></span>
                  <span class="flex items-center gap-1 text-xs text-muted-foreground"><span class="truncate">{commit.author_name}</span> · <time class="shrink-0 tabular-nums" datetime={commit.authored_at} title={formatDateTime(commit.authored_at, $currentLocale)}>{time.format(new Date(commit.authored_at))}</time></span>
                </span>
                <span title={status} class={['shrink-0 font-mono text-[11.5px]', unpushed ? 'font-medium text-amber-700 dark:text-amber-400' : 'text-muted-foreground']}>{#if status}<span class="sr-only">{status} </span>{/if}{#if unpushed}<span aria-hidden="true">↑ </span>{/if}{shortSha(commit.sha)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/each}
    </div>
    {#if commitsQuery.hasNextPage}
      <div class="px-4 pb-4 text-center">
        <Button variant="outline" size="sm" disabled={commitsQuery.isFetchingNextPage} onclick={() => commitsQuery.fetchNextPage()}>{$t('common.loadMore')}</Button>
      </div>
    {/if}
  {/if}
</div>
