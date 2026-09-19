<script lang="ts">
  import AtIcon from 'phosphor-svelte/lib/AtIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GaugeIcon from 'phosphor-svelte/lib/GaugeIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import CpuIcon from 'phosphor-svelte/lib/CpuIcon'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import type { WorkspaceGitStatusView } from '../../api/types'
  import GitStatusInline from './GitStatusInline.svelte'
  import type { SessionMetadataItem } from './sessionMetadata'

  interface Props {
    gitStatus?: WorkspaceGitStatusView
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
  }

  let { gitStatus, metadataItems, metadataSummary }: Props = $props()
  let sessionDetailsOpen = $state(false)
  const fields = [
    { key: 'workspace', icon: FolderIcon, tone: 'text-repository' },
    { key: 'git', icon: GitBranchIcon, tone: 'text-repository' },
    { key: 'client', icon: TerminalWindowIcon, tone: 'text-success' },
    { key: 'model', icon: CpuIcon, tone: 'text-success' },
    { key: 'context', icon: GaugeIcon, tone: 'text-success' },
    { key: 'profile', icon: RobotIcon, tone: 'text-muted-foreground' },
    { key: 'handle', icon: AtIcon, tone: 'text-muted-foreground' },
  ]
  const visibleFields = $derived(fields.flatMap((field) => {
    const item = metadataItems.find((item) => item.key === field.key)
    return item ? [{ ...field, item }] : []
  }))
</script>

<div data-testid="session-metadata" class="relative min-w-0 flex-1">
  <Popover.Root bind:open={sessionDetailsOpen}>
    <Popover.Trigger class="flex h-7 w-full min-w-0 items-center text-left font-mono text-[11px] outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring" aria-label={`Session details: ${metadataSummary}`}>
      <span data-chat-session-details-summary class="block min-w-0 flex-1 truncate">
        {#each visibleFields as field, index (field.key)}
          {#if index > 0}<span class="mx-1.5 inline-block h-2.5 border-l align-middle" aria-hidden="true"></span>{/if}
          <span class={`inline-flex items-center gap-1 align-middle ${field.tone}`} title={field.item.title}>
            <field.icon class="size-[13px] shrink-0" aria-hidden="true" />
            {#if field.key === 'git' && gitStatus}<GitStatusInline {gitStatus} />{:else}<span>{field.item.value}</span>{/if}
          </span>
        {/each}
      </span>
    </Popover.Trigger>
    <Popover.Content side="top" align="start" role="dialog" aria-label="Session details" class="w-[min(24rem,calc(100vw-2rem))] p-3">
      <dl class="space-y-3 text-sm">
        {#each visibleFields as field (field.key)}
          <div class="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2">
            <dt class={`flex items-start justify-center ${field.tone}`}>
              <field.icon class="size-4" aria-label={field.item.label} />
            </dt>
            <dd class="min-w-0 break-words font-mono text-xs" title={field.item.title} aria-label={`${field.item.label}: ${field.item.title}`}>
              {#if field.key === 'git' && gitStatus}
                <span class="flex flex-wrap gap-1"><GitStatusInline {gitStatus} /></span>
                {#if gitStatus.state === 'error'}<span class="text-destructive">{field.item.title}</span>{/if}
              {:else}
                {field.key === 'workspace' ? field.item.title : field.item.value}
              {/if}
            </dd>
          </div>
        {/each}
      </dl>
    </Popover.Content>
  </Popover.Root>
</div>
