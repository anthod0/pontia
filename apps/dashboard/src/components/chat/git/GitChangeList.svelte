<script lang="ts">
  import type { Snippet } from 'svelte'
  import { number, t } from '$lib/i18n'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import type { GitChangedFile } from '../../../api/types'
  import GitChangeRow from './GitChangeRow.svelte'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import MinusIcon from 'phosphor-svelte/lib/MinusIcon'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  import { changeKey, displayChangeGroup, groupChanges, sameChange, splitChangePath } from './gitChanges'

  let { files, selected, onopen, groupActions, touch, busy, onstage, ondiscard }: {
    groupActions: Snippet<[GitChangedFile['group']]>
    touch: boolean; busy: boolean
    onstage: (file: GitChangedFile) => void
    ondiscard: (file: GitChangedFile) => void
    files: GitChangedFile[]
    selected?: { path: string; group: GitChangedFile['group'] } | null
    onopen: (file: GitChangedFile) => void
  } = $props()
  const groups = $derived(groupChanges(files))
  let expanded = $state<string | null>(null)
  let sheetFile = $state<GitChangedFile | null>(null)
</script>

<div class="px-2 py-1">
  {#each groups as group (group.group)}
    <div class="py-0.5">
      {#if groups.length > 1 || group.group === 'conflicted'}
        <div class={['group/label grid h-[26px] grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-2 px-2 text-11 font-medium text-muted-foreground max-lg:h-8', group.group === 'conflicted' && 'text-(--git-conflict)']}>
          {#if group.group === 'conflicted'}<WarningCircleIcon aria-hidden="true" class="size-[13px]" />{:else}<span></span>{/if}
          <span>{$t(group.label)} <span class="font-normal tabular-nums">{$number(group.files.length)}</span></span>
          {@render groupActions(group.group)}
        </div>
      {/if}
      <ul>
        {#each group.files as file (file.group + '\0' + file.path)}
          <GitChangeRow {file} {busy} {touch} label={`${$t(group.label)}: ${file.path}`}
            selected={!!selected && sameChange(file, selected)} expanded={expanded === changeKey(file)}
            expand={open => expanded = open ? changeKey(file) : null} onopen={() => onopen(file)}
            onsheet={() => { expanded = null; sheetFile = file }}
            onstage={() => onstage(file)} ondiscard={() => ondiscard(file)} />
        {/each}
      </ul>
    </div>
  {/each}
</div>

<Sheet.Root open={!!sheetFile} onOpenChange={open => { if (!open) sheetFile = null }}>
  {#if sheetFile}
    {@const file = sheetFile}
    <BottomSheet dismiss={() => sheetFile = null}>
      <Sheet.Title>{splitChangePath(file.path).name}</Sheet.Title>
      <Sheet.Description class="mb-2 font-mono text-xs">{file.path} · {$t(displayChangeGroup(file.group).label)}</Sheet.Description>
      <button class="flex h-12 items-center gap-3 px-2 text-base" disabled={busy} onclick={() => { sheetFile = null; onstage(file) }}>
        {#if file.group === 'staged'}<MinusIcon class="size-5" />{$t('git.unstage')}{:else}<PlusIcon class="size-5" />{$t('git.stage')}{/if}
      </button>
      {#if file.group !== 'staged' && !file.path.endsWith('/')}
        <button class="flex h-12 items-center gap-3 px-2 text-base text-destructive" disabled={busy} onclick={() => { sheetFile = null; ondiscard(file) }}><ArrowCounterClockwiseIcon class="size-5" />{$t('git.discard')}…</button>
      {/if}
    </BottomSheet>
  {/if}
</Sheet.Root>
