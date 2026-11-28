<script lang="ts">
  import { t } from '$lib/i18n'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import { contextUsageRatio } from '$lib/contextUsage'
  import type { SessionView, WorkspaceGitStatusView } from '../../../api/types'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import { gitBranchLabel, type SessionMetadataItem } from '../../../components/chat/sessionMetadata'

  let { session, gitStatus, items, summary, refresh }: {
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    items: SessionMetadataItem[]
    summary: string
    refresh: () => void
  } = $props()
  let open = $state(false)
  const workspace = $derived(items.find(item => item.key === 'workspace')?.value)
  const client = $derived(items.find(item => item.key === 'client')?.value)
  const hasContext = $derived(items.some(item => item.key === 'context'))
  const model = $derived(session.model?.split('/').at(-1))
  const acpAgentId = $derived((session.acp as { agent_id?: string } | undefined)?.agent_id)
  const ratio = $derived(session.context_usage ? contextUsageRatio(session.context_usage) : null)
  const percent = $derived(ratio === null ? null : Math.round(ratio * 100))
</script>

{#snippet separator()}
  <span aria-hidden="true" class="mx-[7px] h-2.5 shrink-0 border-l"></span>
{/snippet}

<button
  type="button"
  class="relative mt-1 flex h-[22px] w-full items-center whitespace-nowrap rounded-item pr-3 pl-3.5 text-[11.5px] leading-4 text-muted-foreground after:absolute after:inset-x-0 after:-top-2 after:bottom-0 active:bg-muted"
  aria-label={$t('chat.detailsSummary', { summary })}
  onclick={() => {
    open = true
    refresh()
  }}
>
  <span class="flex min-w-0 flex-1 items-center overflow-hidden">
    <span class="flex max-w-[28%] shrink-0 items-center gap-1">
      <FolderIcon class="size-[13px] shrink-0" />
      <span class="truncate">{workspace}</span>
    </span>
    {#if gitStatus}
      {@render separator()}
      <span class="flex min-w-0 max-w-[36%] items-center gap-1">
        <GitBranchIcon class="size-[13px] shrink-0" />
        <span class="truncate">{gitBranchLabel(gitStatus, $t)}</span>
      </span>
    {/if}
    {@render separator()}
    <span class="flex min-w-0 items-center gap-1">
      <ClientIcon clientType={session.client_type} {acpAgentId} class="size-[13px]" />
      <span class="truncate">{model || client}</span>
      {#if model && session.reasoning_effort}<span class="shrink-0">· {session.reasoning_effort}</span>{/if}
    </span>
  </span>
  {#if hasContext}
    <span
      class="ml-2.5 flex shrink-0 items-center gap-1 tabular-nums"
      class:text-warning={percent !== null && percent >= 70 && percent < 90}
      class:text-destructive={percent !== null && percent >= 90}
    >
      <svg aria-hidden="true" class="size-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
        {#if percent === null}
          <circle cx="8" cy="8" r="6.5" stroke-dasharray="2 2" />
        {:else}
          <circle cx="8" cy="8" r="6.5" opacity=".25" />
          <circle cx="8" cy="8" r="6.5" pathLength="100" stroke-dasharray="{Math.min(100, percent)} 100" transform="rotate(-90 8 8)" />
        {/if}
      </svg>
      {percent === null ? '…' : percent + '%'}
    </span>
  {/if}
</button>
<Sheet.Root bind:open
  ><BottomSheet dismiss={() => (open = false)} aria-describedby={undefined}
    ><Sheet.Title class="py-3 font-semibold">{$t('chat.details')}</Sheet.Title>
    <dl>
      {#each items as item (item.key)}<div class="flex min-h-11 items-center justify-between gap-3 border-t">
          <dt>{item.label}</dt>
          <dd class="min-w-0 break-all text-right font-mono text-xs">{item.title}</dd>
        </div>{/each}
    </dl></BottomSheet
  ></Sheet.Root
>
