<script lang="ts">
  import { t, number } from '$lib/i18n'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import type { GitChangedFile, GitChangeGroup } from '../../../api/types'
  import { gitStatusTone, groupChanges, parentDir, splitChangePath } from './gitChanges'

  let { files, onopen }: { files: GitChangedFile[]; onopen: (file: GitChangedFile) => void } = $props()

  const groups = $derived(groupChanges(files))
  let collapsed = $state<Partial<Record<GitChangeGroup, boolean>>>({})
  let list = $state<HTMLElement>()

  /** Up and Down move between file rows; Enter opens the focused row as a native button. */
  function keydown(event: KeyboardEvent) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    const rows = [...list!.querySelectorAll<HTMLElement>('[data-git-change-row]')]
    if (!rows.length) return
    event.preventDefault()
    const index = rows.indexOf(document.activeElement as HTMLElement)
    const next = index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1)))
    rows[next].focus()
  }

  /** A move shows `old dir → new dir`; a rename within one directory shows the old name. */
  function dirLabel(file: GitChangedFile): string {
    const dir = parentDir(file.path)
    if (!file.orig_path) return dir
    const from = splitChangePath(file.orig_path)
    return from.dir === dir ? `${from.name} → ${dir || '.'}` : `${from.dir || '.'} → ${dir || '.'}`
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={list} class="px-2 py-1" onkeydown={keydown}>
  {#each groups as group (group.group)}
    <Collapsible.Root open={!collapsed[group.group]} onOpenChange={(open) => { collapsed[group.group] = !open }} class="py-1">
      <Collapsible.Trigger class="group flex h-[30px] w-full items-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring max-lg:h-9">
        <CaretDownIcon class="size-3 transition-transform group-data-[state=closed]:-rotate-90" />
        <span class={['w-3.5 text-center font-mono', group.tone]} aria-hidden="true">{group.symbol}</span>
        <span class="text-foreground">{$t(group.label)}</span>
        <span class="tabular-nums">{$number(group.files.length)}</span>
      </Collapsible.Trigger>
      <Collapsible.Content>
        <ul>
          {#each group.files as file (file.group + '\0' + file.path)}
            {@const parts = splitChangePath(file.path)}
            {@const dir = dirLabel(file)}
            {@const directory = file.path.endsWith('/')}
            <li>
              <button type="button" data-git-change-row data-group={file.group} data-path={file.path}
                class="flex h-[30px] w-full min-w-0 items-center gap-2 rounded-md pr-2 pl-2.5 text-left text-[13px] outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none max-lg:h-10"
                title={file.orig_path ? `${file.orig_path} → ${file.path}` : file.path}
                aria-label={`${$t(group.label)}: ${file.path}`}
                disabled={directory}
                onclick={() => onopen(file)}>
                <span class={['w-3.5 shrink-0 text-center font-mono text-[11px] font-medium', gitStatusTone(file.status)]} aria-hidden="true">{file.status}</span>
                <span class={['max-w-[60%] shrink-0 truncate', file.status === 'D' && 'text-muted-foreground line-through']}>{parts.name}</span>
                <span class="min-w-0 flex-1 truncate text-left text-xs text-muted-foreground" dir="rtl"><bdi>{dir}</bdi></span>
                <span class="flex shrink-0 gap-1.5 font-mono text-[11px] tabular-nums">
                  {#if directory}<span class="text-muted-foreground">{$t('git.directory')}</span>
                  {:else if file.binary}<span class="text-muted-foreground">{$t('git.binary')}</span>
                  {:else}
                    {#if file.additions}<span class="text-success">+{file.additions}</span>{/if}
                    {#if file.deletions}<span class="text-destructive">−{file.deletions}</span>{/if}
                  {/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </Collapsible.Content>
    </Collapsible.Root>
  {/each}
</div>
