<script lang="ts">
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

<button type="button" class="flex h-7 w-full min-w-0 items-center justify-start bg-transparent px-0 text-sm text-muted-foreground outline-none hover:bg-transparent hover:text-foreground focus-visible:text-foreground" aria-haspopup="dialog" aria-expanded={sessionDetailsOpen} aria-label={`Session details: ${metadataSummary}`} onclick={() => (sessionDetailsOpen = !sessionDetailsOpen)}>
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
</button>
{#if sessionDetailsOpen}
  <div role="dialog" aria-label="Session details" class="absolute bottom-full left-0 z-20 mb-2 w-[min(20rem,calc(100vw-2rem))] rounded-lg border bg-popover p-3 text-popover-foreground shadow-md">
    <div class="mb-2 text-sm font-medium">Session details</div>
    <dl class="space-y-2 text-sm">
      {#each metadataItems as item (item.key)}
        <div class="grid grid-cols-[5.5rem_minmax(0,1fr)] gap-2">
          <dt class="text-muted-foreground">{item.label}</dt>
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
  </div>
{/if}
