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
    workspaces: WorkspaceView[]
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
  }

  let { session, gitStatus, workspaces, metadataItems, metadataSummary }: Props = $props()
  let sessionDetailsOpen = $state(false)
</script>

<div data-testid="session-metadata" class="relative min-w-0 flex-1">
  <Popover.Root bind:open={sessionDetailsOpen}>
    <Popover.Trigger class="flex h-7 w-full min-w-0 items-center justify-start bg-transparent px-0 text-sm text-muted-foreground outline-none hover:bg-transparent hover:text-foreground focus-visible:text-foreground" aria-label={`Session details: ${metadataSummary}`}>
      <span data-chat-session-details-summary class="block min-w-0 flex-1 truncate text-left">
        <span class="inline-flex min-w-0 items-center gap-1 align-middle">
          <Folder class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" />
          <span class="min-w-0 truncate">{sessionWorkspaceTitle(session, workspaces)}</span>
        </span>
        {#if gitStatus}
          <span> ·</span>{' '}
          <span class="inline-flex items-center gap-1 align-middle"><GitBranch class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" /><GitStatusInline {gitStatus} /></span>
        {/if}
        {#if sessionContextUsageLabel(session)}
          <span> ·</span>{' '}
          <span class="inline-flex items-center gap-1 align-middle"><Gauge class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" /><span>{sessionContextUsageLabel(session)}</span></span>
        {/if}
        <span> ·</span>{' '}
        <span class="inline-flex items-center gap-1 align-middle"><Terminal class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" /><span>{session.client_type}</span></span>
        {#if sessionProfileTitle(session)}
          <span> ·</span>{' '}
          <span class="inline-flex items-center gap-1 align-middle"><Bot class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" /><span>{sessionProfileTitle(session)}</span></span>
        {/if}
        {#if sessionHandleTitle(session)}
          <span> ·</span>{' '}
          <span class="inline-flex items-center gap-1 align-middle"><AtSign class="hidden size-4 shrink-0 sm:inline" aria-hidden="true" /><span>{sessionHandleTitle(session)}</span></span>
        {/if}
      </span>
    </Popover.Trigger>
    <Popover.Content side="top" align="start" role="dialog" aria-label="Session details" class="w-[min(20rem,calc(100vw-2rem))] p-3">
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
            <dd class="min-w-0 truncate" title={item.title} aria-label={`${item.label}: ${item.title}`}>
              {#if item.key === 'git' && gitStatus}<GitStatusInline {gitStatus} />{:else}{item.value}{/if}
            </dd>
          </div>
        {/each}
      </dl>
    </Popover.Content>
  </Popover.Root>
</div>
