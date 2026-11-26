<script lang="ts">
  import { t } from '$lib/i18n'
  import type { GitChangedFile, GitCommitFile } from '../../../api/types'
  import { changeDirLabel, gitStatusTone, splitChangePath } from './gitChanges'

  let { file, rowKey, label, selected = false, onopen }: {
    file: GitChangedFile | GitCommitFile
    rowKey: string
    label: string
    selected?: boolean
    onopen: () => void
  } = $props()
  const parts = $derived(splitChangePath(file.path))
  const dir = $derived(changeDirLabel(file))
  const directory = $derived(file.path.endsWith('/'))
</script>

<button type="button" data-git-row={rowKey}
  class={['grid h-[30px] w-full min-w-0 grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 text-left text-[13px] outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset max-lg:h-11', selected && 'bg-selected']}
  title={file.orig_path ? `${file.orig_path} → ${file.path}` : file.path}
  aria-label={label}
  disabled={directory}
  onclick={onopen}>
  <span class={['text-center font-mono text-[11px] font-medium', gitStatusTone(file.status)]} aria-hidden="true">{file.status}</span>
  <span class="flex min-w-0 items-baseline gap-2">
    <span class={['max-w-[68%] shrink-0 truncate', file.status === 'D' && 'text-muted-foreground']}>{parts.name}</span>
    <span class="min-w-0 flex-1 truncate text-left text-xs text-muted-foreground" dir="rtl"><bdi>{dir}</bdi></span>
  </span>
  <span class="flex shrink-0 gap-1.5 font-mono text-[11px] tabular-nums">
    {#if directory}<span class="text-muted-foreground">{$t('git.directory')}</span>
    {:else if file.binary}<span class="text-muted-foreground">{$t('git.binary')}</span>
    {:else}
      {#if file.additions}<span class="text-git-added">+{file.additions}</span>{/if}
      {#if file.deletions}<span class="text-git-deleted">−{file.deletions}</span>{/if}
    {/if}
  </span>
</button>
