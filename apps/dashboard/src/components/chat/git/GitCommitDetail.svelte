<script lang="ts">
  import { currentLocale, number, t } from '$lib/i18n'
  import { formatDateTime } from '$lib/format'
  import { copyText } from '$lib/copyText'
  import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon'
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
  import { changeDirLabel, gitStatusTone, moveRowFocus, shortSha, splitChangePath } from './gitChanges'

  /** One commit: its message and author, and the files it changed against its first parent. */
  let { device, workspaceId, sha, onback, onopen }: {
    device: DeviceRuntime
    workspaceId: string
    sha: string
    onback?: () => void
    onopen: (file: GitCommitFile) => void
  } = $props()

  // The panel recreates this view for each commit and device.
  // svelte-ignore state_referenced_locally
  const commitQuery = device.workspaceQueries.createWorkspaceGitCommitQuery(() => workspaceId, () => sha)
  const commit = $derived(commitQuery.data)
  let list = $state<HTMLElement>()
  let copied = $state(false)

  async function copySha() {
    copied = await copyText(sha)
    if (copied) setTimeout(() => { copied = false }, 1500)
  }
</script>

<header class="flex h-12 shrink-0 items-center gap-1 border-b px-2 max-lg:h-14">
  {#if onback}  <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" onclick={onback} aria-label={$t('git.backToHistory')} title={$t('git.backToHistory')}><ArrowLeftIcon class="size-4 max-lg:size-5" /></Button>{/if}
  <h2 class="min-w-0 flex-1 truncate px-1 text-sm font-medium text-heading">{$t('git.commitTitle', { sha: shortSha(sha) })}</h2>
  <Button variant="ghost" size="icon-sm" class="rounded-full p-0 max-lg:size-11" onclick={copySha} aria-label={$t('git.copySha')} title={$t('git.copySha')}>
    {#if copied}<CheckIcon class="size-4 text-success max-lg:size-5" />{:else}<CopyIcon class="size-4 max-lg:size-5" />{/if}
  </Button>
</header>

<div class="min-h-0 flex-1 overflow-auto overscroll-contain">
  {#if commitQuery.isError}
    <Empty.Root role="alert" class="min-h-0 p-8">
      <Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-destructive/10 text-destructive"><WarningCircleIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('git.commitFailed')}</Empty.Title>
        <Empty.Description class="break-words font-mono text-xs">{commitQuery.error.message}</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button variant="outline" size="sm" onclick={() => commitQuery.refetch()}><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.retry')}</Button>
      </Empty.Content>
    </Empty.Root>
  {:else if !commit}
    <div role="status" aria-label={$t('common.loading')} class="space-y-2 p-4">
      <Skeleton class="h-4 w-[70%]" />
      <Skeleton class="h-3 w-[45%]" />
      <div class="pt-3"></div>
      {#each [60, 80, 52, 74] as width, index (index)}<Skeleton class="h-3" style="width: {width}%" />{/each}
    </div>
  {:else}
    <section class="space-y-2 border-b px-4 py-3" aria-label={$t('git.commitMessage')}>
      <p class="text-sm font-medium break-words text-heading">{commit.subject || $t('git.noSubject')}</p>
      {#if commit.body}<p class="text-xs leading-5 break-words whitespace-pre-wrap text-muted-foreground">{commit.body}</p>{/if}
      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs">
        <dt class="text-muted-foreground">{$t('git.author')}</dt>
        <dd class="min-w-0 truncate" title={commit.author_email}>{commit.author_name}</dd>
        <dt class="text-muted-foreground">{$t('git.date')}</dt>
        <dd><time datetime={commit.authored_at}>{formatDateTime(commit.authored_at, $currentLocale)}</time></dd>
        <dt class="text-muted-foreground">{$t('git.commitSha')}</dt>
        <dd class="min-w-0 truncate font-mono" title={commit.sha}>{commit.sha}</dd>
        {#if commit.parents.length}
          <dt class="text-muted-foreground">{$t('git.parents')}</dt>
          <dd class="font-mono">{commit.parents.map(shortSha).join(' ')}</dd>
        {/if}
      </dl>
    </section>

    <div class="flex items-center gap-2 px-4 pt-3 pb-1 text-xs font-medium text-muted-foreground">
      <span class="text-foreground">{$t('git.changedFiles')}</span>
      <span class="tabular-nums">{$number(commit.total)}</span>
      {#if commit.parents.length > 1}<span class="ml-auto font-normal">{$t('git.mergeComparedWithFirstParent')}</span>{/if}
    </div>
    {#if commit.files.length}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul bind:this={list} class="px-2 pb-2" onkeydown={(event) => moveRowFocus(event, list!)}>
        {#each commit.files as file (file.path)}
          {@const parts = splitChangePath(file.path)}
          <li>
            <button type="button" data-git-row={file.path} onclick={() => onopen(file)}
              class="flex h-[30px] w-full min-w-0 items-center gap-2 rounded-md pr-2 pl-2.5 text-left text-[13px] outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring max-lg:h-10"
              title={file.orig_path ? `${file.orig_path} → ${file.path}` : file.path}
              aria-label={file.path}>
              <span class={['w-3.5 shrink-0 text-center font-mono text-[11px] font-medium', gitStatusTone(file.status)]} aria-hidden="true">{file.status}</span>
              <span class={['max-w-[60%] shrink-0 truncate', file.status === 'D' && 'text-muted-foreground line-through']}>{parts.name}</span>
              <span class="min-w-0 flex-1 truncate text-left text-xs text-muted-foreground" dir="rtl"><bdi>{changeDirLabel(file)}</bdi></span>
              <span class="flex shrink-0 gap-1.5 font-mono text-[11px] tabular-nums">
                {#if file.binary}<span class="text-muted-foreground">{$t('git.binary')}</span>
                {:else}
                  {#if file.additions}<span class="text-success">+{file.additions}</span>{/if}
                  {#if file.deletions}<span class="text-destructive">−{file.deletions}</span>{/if}
                {/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if commit.truncated}
        <p class="px-4 pb-4 text-center text-xs text-muted-foreground">{$t('git.listTruncated', { shown: commit.files.length, total: commit.total })}</p>
      {/if}
    {:else}
      <Empty.Root class="min-h-0 p-8">
        <Empty.Header>
          <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><FileDashedIcon class="size-6" /></Empty.Media>
          <Empty.Title>{$t('git.noFilesChanged')}</Empty.Title>
        </Empty.Header>
      </Empty.Root>
    {/if}
  {/if}
</div>
