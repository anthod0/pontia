<script lang="ts">
  import { currentLocale, t } from '$lib/i18n'
  import { formatDateTime } from '$lib/format'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import GitMergeIcon from 'phosphor-svelte/lib/GitMergeIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitCommitFile } from '../../../api/types'
  import GitCommitDetail from './GitCommitDetail.svelte'
  import { shortSha } from './gitChanges'

  let { device, workspaceId, ref, upstream, selected, inline, selectFirst = false, oncommit, onfile }: {
    device: DeviceRuntime
    workspaceId: string
    ref: string | null
    upstream?: string | null
    selected: string | null
    inline: boolean
    selectFirst?: boolean
    oncommit: (sha: string) => void
    onfile: (sha: string, file: GitCommitFile, files: GitCommitFile[]) => void
  } = $props()

  // The panel recreates this view for each device and Workspace.
  // svelte-ignore state_referenced_locally
  const queries = device.workspaceQueries
  const commitsQuery = queries.createWorkspaceGitCommitsQuery(() => workspaceId, () => ref)
  const commits = $derived(commitsQuery.data?.pages.flatMap(page => page.commits) ?? [])
  const date = $derived(new Intl.DateTimeFormat($currentLocale, { dateStyle: 'medium' }))
  const time = $derived(new Intl.DateTimeFormat($currentLocale, { hour: '2-digit', minute: '2-digit' }))
  const firstPushed = $derived(commits.find(commit => commit.pushed !== false)?.sha)
  const days = $derived.by(() => {
    const groups: { label: string; commits: typeof commits }[] = []
    const today = new Date()
    const yesterday = new Date(today)
    yesterday.setDate(today.getDate() - 1)
    const relativeDay = new Intl.RelativeTimeFormat($currentLocale, { numeric: 'auto' })
    for (const commit of commits) {
      const authored = new Date(commit.authored_at)
      const day = authored.toDateString()
      const label = day === today.toDateString() ? relativeDay.format(0, 'day') : day === yesterday.toDateString() ? relativeDay.format(-1, 'day') : date.format(authored)
      if (groups.at(-1)?.label !== label) groups.push({ label, commits: [] })
      groups.at(-1)!.commits.push(commit)
    }
    return groups
  })

  $effect(() => {
    if (selectFirst && !selected && commits[0]) oncommit(commits[0].sha)
  })

  export function refresh() { if (ref) return commitsQuery.refetch() }
</script>

{#if !ref}
  <p class="px-10 py-3 text-xs text-muted-foreground">{$t('git.noCommits')}</p>
{:else if commitsQuery.isError && !commits.length}
  <Alert.Root variant="destructive" class="m-3 w-auto"><WarningCircleIcon /><Alert.Title>{$t('git.historyFailed')}</Alert.Title><Alert.Description><span class="font-mono text-xs break-words text-foreground">{commitsQuery.error.message}</span><Button variant="outline" size="sm" class="mt-2" onclick={() => commitsQuery.refetch()}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button></Alert.Description></Alert.Root>
{:else if !commitsQuery.data}
  <div role="status" aria-label={$t('common.loading')} class="space-y-3 p-4">{#each [70, 52, 64, 45, 58] as width}<div class="space-y-1.5"><Skeleton class="h-3.5" style="width: {width}%" /><Skeleton class="h-3 w-[40%]" /></div>{/each}</div>
{:else if !commits.length}
  <p class="px-10 py-3 text-xs text-muted-foreground">{$t('git.noCommits')}</p>
{:else}
  <div class="relative px-2 pb-2 before:absolute before:top-[36px] before:bottom-[24px] before:left-[21px] before:w-px before:bg-border">
    {#each days as day (day.commits[0].sha)}
      <h3 class="grid h-7 grid-cols-[16px_minmax(0,1fr)] items-center gap-x-2 px-2 text-xs font-medium text-muted-foreground"><span></span><span>{day.label}</span></h3>
      <ul aria-label={day.label}>
        {#each day.commits as commit (commit.sha)}
          {@const active = selected === commit.sha}
          <li class="relative">
            <button type="button" data-git-row={commit.sha} onclick={() => oncommit(commit.sha)} aria-expanded={inline ? active : undefined}
              class={['group relative grid h-11 w-full min-w-0 grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 rounded-item px-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset max-lg:h-[52px]', !inline && active && 'bg-selected']}>
              <span aria-hidden="true" class={['relative z-10 mx-auto size-2 rounded-full border border-current', commit.pushed === false ? 'text-primary' : 'text-muted-foreground/60', commit.parents.length > 1 ? 'bg-surface' : 'bg-current']}>{#if commit.parents.length > 1}<GitMergeIcon class="absolute top-1/2 left-1/2 size-[13px] -translate-x-1/2 -translate-y-1/2 bg-surface" />{/if}</span>
              <span class="min-w-0">
                <span class={['block truncate text-13', active && inline && 'font-medium text-heading']}>{commit.subject || $t('git.noSubject')}</span>
                <span class="flex items-center gap-1 text-xs text-muted-foreground"><span class="truncate">{commit.author_name}</span> · <time class="shrink-0 tabular-nums" datetime={commit.authored_at} title={formatDateTime(commit.authored_at, $currentLocale)}>{time.format(new Date(commit.authored_at))}</time>{#if commit.sha === firstPushed && upstream}<span class="ml-1 h-4 truncate rounded-chip border px-1.5 font-mono text-10 leading-[14px]">{upstream}</span>{/if}</span>
              </span>
              {#if !(inline && active)}<span class="font-mono text-11 text-muted-foreground group-hover:text-foreground">{shortSha(commit.sha)}</span>{/if}
            </button>
            {#if inline && active}
              <div class="relative z-10 ml-8 pb-2"><GitCommitDetail {device} {workspaceId} sha={commit.sha} inline onopen={(file, files) => onfile(commit.sha, file, files)} onparent={oncommit} /></div>
            {/if}
          </li>
        {/each}
      </ul>
    {/each}
  </div>
  {#if commitsQuery.hasNextPage}<div class="px-4 pb-4 text-center"><Button variant="outline" size="sm" disabled={commitsQuery.isFetchingNextPage} onclick={() => commitsQuery.fetchNextPage()}>{$t('common.loadMore')}</Button></div>{/if}
{/if}
