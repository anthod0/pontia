<script lang="ts">
  import { t, number } from '$lib/i18n'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import type { GitChangedFile, GitChangeGroup } from '../../../api/types'
  import { changeDirLabel, changeKey, gitStatusTone, groupChanges, moveRowFocus, splitChangePath } from './gitChanges'

  let { files, onopen }: { files: GitChangedFile[]; onopen: (file: GitChangedFile) => void } = $props()

  const groups = $derived(groupChanges(files))
  let collapsed = $state<Partial<Record<GitChangeGroup, boolean>>>({})
  let list = $state<HTMLElement>()

</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={list} class="px-2 py-1" onkeydown={(event) => moveRowFocus(event, list!)}>
  {#each groups as group (group.group)}
    <Collapsible.Root open={!collapsed[group.group]} onOpenChange={(open) => { collapsed[group.group] = !open }} class="py-1">
      <Collapsible.Trigger class={['group sticky top-0 z-10 flex h-[30px] w-full items-center gap-1.5 rounded-md bg-surface px-2 text-xs font-medium text-muted-foreground outline-none hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset max-lg:h-9', group.group === 'conflicted' && 'text-destructive']}>
        <CaretDownIcon class="size-3 transition-transform group-data-[state=closed]:-rotate-90" />
        {#if group.group === 'conflicted'}<WarningCircleIcon aria-hidden="true" class="size-3.5" />{/if}
        <span>{$t(group.label)}</span>
        <span class="font-normal tabular-nums">{$number(group.files.length)}</span>
      </Collapsible.Trigger>
      <Collapsible.Content>
        <ul>
          {#each group.files as file (file.group + '\0' + file.path)}
            {@const parts = splitChangePath(file.path)}
            {@const dir = changeDirLabel(file)}
            {@const directory = file.path.endsWith('/')}
            <li>
              <button type="button" data-git-row={changeKey(file)}
                class="flex h-[30px] w-full min-w-0 items-center gap-2 rounded-lg pr-2 pl-[22px] text-left text-[13px] outline-none hover:bg-muted focus-visible:bg-muted focus-visible:ring-2 focus-visible:ring-primary/55 focus-visible:ring-inset disabled:pointer-events-none max-lg:h-11"
                title={file.orig_path ? `${file.orig_path} → ${file.path}` : file.path}
                aria-label={`${$t(group.label)}: ${file.path}`}
                disabled={directory}
                onclick={() => onopen(file)}>
                <span class={['w-3.5 shrink-0 text-center font-mono text-[11px] font-medium', gitStatusTone(file.status)]} aria-hidden="true">{file.status}</span>
                <span class={['max-w-[60%] shrink-0 truncate', file.status === 'D' && 'text-muted-foreground']}>{parts.name}</span>
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
