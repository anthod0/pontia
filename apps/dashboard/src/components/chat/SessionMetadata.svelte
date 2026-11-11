<script lang="ts">
  import AtIcon from 'phosphor-svelte/lib/AtIcon'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GaugeIcon from 'phosphor-svelte/lib/GaugeIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import CpuIcon from 'phosphor-svelte/lib/CpuIcon'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import type { SessionView, WorkspaceGitStatusView } from '../../api/types'
  import ClientIcon from './ClientIcon.svelte'
  import GitStatusInline from './GitStatusInline.svelte'
  import type { SessionMetadataItem } from './sessionMetadata'

  interface Props {
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
  }

  let { session, gitStatus, metadataItems, metadataSummary }: Props = $props()
  const acpAgentId = $derived((session.acp as { agent_id?: string } | undefined)?.agent_id)
  let sessionDetailsOpen = $state(false)
  const fields = [
    { key: 'client', icon: null },
    { key: 'workspace', icon: FolderIcon },
    { key: 'git', icon: GitBranchIcon },
    { key: 'model', icon: CpuIcon },
    { key: 'context', icon: GaugeIcon },
    { key: 'profile', icon: RobotIcon },
    { key: 'handle', icon: AtIcon },
  ]
  const visibleFields = $derived(fields.flatMap((field) => {
    const item = metadataItems.find((item) => item.key === field.key)
    return item ? [{ ...field, item }] : []
  }))
</script>

<div data-testid="session-metadata" class="relative min-w-0 flex-1">
  <Popover.Root bind:open={sessionDetailsOpen}>
    <Popover.Trigger class="flex h-7 w-fit max-w-full min-w-0 items-center rounded-md px-1.5 text-left font-mono text-[11px] outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring" aria-label={`Session details: ${metadataSummary}`}>
      <span data-chat-session-details-summary class="block min-w-0 flex-1 truncate">
        {#each visibleFields as field, index (field.key)}
          {#if index > 0}<span class="mx-1.5 inline-block h-2.5 border-l align-middle" aria-hidden="true"></span>{/if}
          <span class="inline-flex items-center gap-1 align-middle text-muted-foreground" title={field.item.title}>
            {#if field.key === 'client'}
              <ClientIcon clientType={session.client_type} {acpAgentId} class="size-[13px]" />
            {:else if field.icon && field.key !== 'model' && field.key !== 'context'}
              <field.icon class="size-[13px] shrink-0" aria-hidden="true" />
            {/if}
            {#if field.key === 'git' && gitStatus}<GitStatusInline {gitStatus} />{:else}<span>{field.item.value}</span>{/if}
          </span>
        {/each}
      </span>
    </Popover.Trigger>
    <Popover.Content side="top" align="start" role="dialog" aria-label="Session details" class="w-[min(24rem,calc(100vw-2rem))] p-3">
      <dl class="space-y-3 text-sm">
        {#each visibleFields as field (field.key)}
          <div class="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2">
            <dt class="flex items-start justify-center text-muted-foreground">
              {#if field.key === 'client'}
                <ClientIcon clientType={session.client_type} {acpAgentId} class="size-4" />
                <span class="sr-only">{field.item.label}</span>
              {:else if field.icon}
                <field.icon class="size-4" aria-label={field.item.label} />
              {/if}
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
