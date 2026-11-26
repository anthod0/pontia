<script lang="ts">
  import { number, t } from '$lib/i18n'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import type { GitChangedFile } from '../../../api/types'
  import GitFileRow from './GitFileRow.svelte'
  import { changeKey, groupChanges, sameChange } from './gitChanges'

  let { files, selected, onopen }: {
    files: GitChangedFile[]
    selected?: { path: string; group: GitChangedFile['group'] } | null
    onopen: (file: GitChangedFile) => void
  } = $props()
  const groups = $derived(groupChanges(files))
</script>

<div class="px-2 py-1">
  {#each groups as group (group.group)}
    {@const showHeading = groups.length > 1 || group.group === 'conflicted'}
    <div class="py-0.5">
      {#if showHeading}
        <div class={['grid h-[26px] grid-cols-[16px_minmax(0,1fr)] items-center gap-x-2 px-2 text-11 font-medium text-muted-foreground max-lg:h-8', group.group === 'conflicted' && 'text-(--git-conflict)']}>
          {#if group.group === 'conflicted'}<WarningCircleIcon aria-hidden="true" class="size-[13px]" />{:else}<span></span>{/if}
          <span>{$t(group.label)} <span class="font-normal tabular-nums">{$number(group.files.length)}</span></span>
        </div>
      {/if}
      <ul>
        {#each group.files as file (file.group + '\0' + file.path)}
          <li>
            <GitFileRow {file} rowKey={changeKey(file)} label={`${$t(group.label)}: ${file.path}`}
              selected={!!selected && sameChange(file, selected)} onopen={() => onopen(file)} />
          </li>
        {/each}
      </ul>
    </div>
  {/each}
</div>
