<script lang="ts">
  import { t } from '$lib/i18n'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CircleIcon from 'phosphor-svelte/lib/CircleIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import type { ManagedPlanEntry } from '../../../api/types'

  let { entries, connected = false }: { entries: ManagedPlanEntry[]; connected?: boolean } = $props()
</script>

<div class="relative flex min-w-0 gap-3 py-1.5">
  <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground">
    <CircleIcon class="size-4" aria-hidden="true" />
  </span>
  {#if connected}
    <span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>
  {/if}
  <div class="min-w-0 flex-1 pt-0.5 text-sm leading-5">
    <div class="font-medium text-foreground/75">{$t('chat.plan')}</div>
    {#if entries.length}
      <ul class="mt-1 space-y-1 text-muted-foreground">
        {#each entries as entry, index (`${entry.content}:${index}`)}
          <li class="flex min-w-0 gap-2" aria-label={`${entry.content}: ${entry.status}`}>
            {#if entry.status === 'completed'}
              <CheckIcon class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            {:else if entry.status === 'in_progress'}
              <SpinnerGapIcon class="mt-0.5 size-4 shrink-0 animate-spin" aria-hidden="true" />
            {:else}
              <CircleIcon class="mt-1 size-3 shrink-0" aria-hidden="true" />
            {/if}
            <span class="break-words">{entry.content}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
