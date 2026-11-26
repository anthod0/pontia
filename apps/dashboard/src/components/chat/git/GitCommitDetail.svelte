<script lang="ts">
  import { currentLocale, number, t } from '$lib/i18n'
  import { formatDateTime } from '$lib/format'
  import { copyText } from '$lib/copyText'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import FileDashedIcon from 'phosphor-svelte/lib/FileDashedIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import type { DeviceRuntime } from '../../../device/runtime'
  import type { GitCommitFile } from '../../../api/types'
  import GitFileRow from './GitFileRow.svelte'
  import { shortSha } from './gitChanges'

  let { device, workspaceId, sha, inline = false, onopen, onparent }: {
    device: DeviceRuntime
    workspaceId: string
    sha: string
    inline?: boolean
    onopen: (file: GitCommitFile, files: GitCommitFile[]) => void
    onparent?: (sha: string) => void
  } = $props()

  // The panel recreates this view for each device and commit.
  // svelte-ignore state_referenced_locally
  const commitQuery = device.workspaceQueries.createWorkspaceGitCommitQuery(() => workspaceId, () => sha)
  const commit = $derived(commitQuery.data)
  const additions = $derived(commit?.files.reduce((total, file) => total + (file.additions ?? 0), 0) ?? 0)
  const deletions = $derived(commit?.files.reduce((total, file) => total + (file.deletions ?? 0), 0) ?? 0)
  let copied = $state(false)

  async function copySha() {
    copied = await copyText(sha)
    if (copied) setTimeout(() => { copied = false }, 1500)
  }
</script>

{#if !inline}
  <header class="flex h-[52px] shrink-0 items-center gap-2 border-b px-2">
    <h2 class="min-w-0 flex-1 px-1 text-13 font-medium text-heading">{$t('git.commit')}</h2>
    <button type="button" class="flex h-6 items-center gap-1 rounded-chip bg-muted px-2 font-mono text-11 hover:text-foreground" onclick={copySha} title={sha}>
      {shortSha(sha)}{#if copied}<CheckIcon class="size-3 text-git-added" />{:else}<CopyIcon class="size-3" />{/if}
    </button>
  </header>
{/if}

<div class={inline ? 'rounded-item bg-[color-mix(in_srgb,var(--muted)_55%,var(--surface))] px-3 py-2.5' : 'min-h-0 flex-1 overflow-auto overscroll-contain'}>
  {#if commitQuery.isError}
    <Empty.Root role="alert" class="min-h-0 p-6">
      <Empty.Header><Empty.Media class="size-10 rounded-item bg-destructive/10 text-destructive"><WarningCircleIcon class="size-5" /></Empty.Media><Empty.Title>{$t('git.commitFailed')}</Empty.Title><Empty.Description class="break-words font-mono text-xs">{commitQuery.error.message}</Empty.Description></Empty.Header>
      <Empty.Content><Button variant="outline" size="sm" onclick={() => commitQuery.refetch()}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button></Empty.Content>
    </Empty.Root>
  {:else if !commit}
    <div role="status" aria-label={$t('common.loading')} class="space-y-2 p-3"><Skeleton class="h-4 w-[70%]" /><Skeleton class="h-3 w-[45%]" />{#each [60, 80, 52] as width}<Skeleton class="h-3" style="width: {width}%" />{/each}</div>
  {:else}
    <section class={inline ? 'space-y-2' : 'space-y-2 border-b px-4 py-3'} aria-label={$t('git.commitMessage')}>
      {#if !inline}<p class="text-sm font-medium break-words text-heading">{commit.subject || $t('git.noSubject')}</p>{/if}
      {#if commit.body}<p class={['text-xs leading-5 break-words whitespace-pre-wrap text-muted-foreground', inline && 'line-clamp-3']}>{commit.body}</p>{/if}
      <div class="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground">
        {#if !inline}<span class="grid size-[18px] place-items-center rounded-full bg-muted text-10 font-medium text-foreground">{commit.author_name.slice(0, 1).toUpperCase()}</span><span>{commit.author_name}</span><span>·</span>{/if}
        {#if inline}
          <button type="button" class="flex h-5 items-center gap-1 rounded-chip bg-surface px-2 font-mono text-11 text-foreground hover:bg-card" onclick={copySha} title={sha}>{shortSha(sha)}{#if copied}<CheckIcon class="size-3 text-git-added" />{:else}<CopyIcon class="size-3" />{/if}</button><span>·</span>
        {/if}
        {#if commit.parents.length}
          <span>{$t('git.parents')}</span>
          {#each commit.parents as parent}<button type="button" class="font-mono text-primary hover:underline" onclick={() => onparent?.(parent)}>{shortSha(parent)}</button>{/each}
          <span>·</span>
        {/if}
        <time datetime={commit.authored_at} title={formatDateTime(commit.authored_at, $currentLocale)}>{formatDateTime(commit.authored_at, $currentLocale)}</time>
      </div>
    </section>

    <div class={['flex items-center gap-2 text-xs font-medium', inline ? 'pt-3 pb-1' : 'px-4 pt-3 pb-1']}>
      <span>{$t('git.fileCount', { count: commit.total })}</span>
      <span class="font-mono text-11 text-git-added">+{additions}</span>
      <span class="font-mono text-11 text-git-deleted">−{deletions}</span>
      {#if commit.parents.length > 1}<span class="ml-auto font-normal text-muted-foreground">{$t('git.mergeComparedWithFirstParent')}</span>{/if}
    </div>
    {#if commit.files.length}
      <ul class={inline ? '' : 'px-2 pb-2'}>
        {#each commit.files as file (file.path)}<li><GitFileRow {file} rowKey={`${sha}:${file.path}`} label={file.path} onopen={() => onopen(file, commit.files)} /></li>{/each}
      </ul>
      {#if commit.truncated}<p class="px-4 pb-4 text-center text-xs text-muted-foreground">{$t('git.listTruncated', { shown: commit.files.length, total: commit.total })}</p>{/if}
    {:else}
      <Empty.Root class="min-h-0 p-6"><Empty.Header><Empty.Media class="size-10 rounded-item bg-muted text-muted-foreground"><FileDashedIcon class="size-5" /></Empty.Media><Empty.Title>{$t('git.noFilesChanged')}</Empty.Title></Empty.Header></Empty.Root>
    {/if}
  {/if}
</div>
