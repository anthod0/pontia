<script lang="ts">
  import { AtSign, Bot, Folder, Gauge, GitBranch, Terminal } from '@lucide/svelte'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import type { SessionView, WorkspaceGitStatusView, WorkspaceView } from '../../api/types'
  import GitStatusInline from './GitStatusInline.svelte'
  import {
    sessionContextUsageLabel,
    sessionHandleTitle,
    sessionProfileTitle,
    sessionWorkspaceTitle,
    type SessionMetadataItem,
  } from './sessionMetadata'

  interface Props {
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    gitStatusErrors?: Record<string, string | null | undefined>
    workspaces: WorkspaceView[]
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
  }

  let { session, gitStatus, workspaces, metadataItems, metadataSummary }: Props = $props()
  let sessionDetailsOpen = $state(false)
</script>

<Popover.Root bind:open={sessionDetailsOpen}>
  <Popover.Trigger class="flex h-7 w-full min-w-0 items-center justify-start bg-transparent px-0 text-sm text-muted-foreground outline-none hover:bg-transparent hover:text-foreground focus-visible:text-foreground" aria-label={`Session details: ${metadataSummary}`}>
    <span data-chat-session-details-summary class="block min-w-0 flex-1 truncate text-left">
      <span>{sessionWorkspaceTitle(session, workspaces)}</span>
      {#if gitStatus}
        <span> ·</span>{' '}<GitStatusInline {gitStatus} />
      {/if}
      {#if sessionContextUsageLabel(session)}
        <span> · {sessionContextUsageLabel(session)}</span>
      {/if}
      <span> · {session.client_type}</span>
      {#if sessionProfileTitle(session)}<span> · {sessionProfileTitle(session)}</span>{/if}
      {#if sessionHandleTitle(session)}<span> · {sessionHandleTitle(session)}</span>{/if}
    </span>
  </Popover.Trigger>
  <Popover.Content side="top" align="start" role="dialog" aria-label="Session details" portalProps={{ disabled: true }} class="w-[min(20rem,calc(100vw-2rem))] p-3">
    <dl class="space-y-2 text-sm">
      {#each metadataItems as item (item.key)}
        <div class="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2">
          <dt class="flex items-center justify-center text-muted-foreground">
            {#if item.key === 'workspace'}
              <Folder class="size-4" aria-label="Workspace" />
            {:else if item.key === 'git'}
              <GitBranch class="size-4" aria-label="Git" />
            {:else if item.key === 'context'}
              <Gauge class="size-4" aria-label="Usage" />
            {:else if item.key === 'client'}
              <Terminal class="size-4" aria-label="Client" />
            {:else if item.key === 'profile'}
              <Bot class="size-4" aria-label="Profile" />
            {:else if item.key === 'handle'}
              <AtSign class="size-4" aria-label="Handle" />
            {/if}
          </dt>
          <dd class="min-w-0 truncate" title={item.title}>
            {#if item.key === 'git' && gitStatus}
              <GitStatusInline {gitStatus} />
            {:else}
              {item.value}
            {/if}
          </dd>
        </div>
      {/each}
    </dl>
  </Popover.Content>
</Popover.Root>
