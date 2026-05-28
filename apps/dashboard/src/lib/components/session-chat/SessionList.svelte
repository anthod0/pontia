<script lang="ts">
  import { MessageCircle } from '@lucide/svelte'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import type { SessionView } from '../../../api/types'
  import { formatDateTime } from '../../../components/tasks/format'
  import { isTerminalChatSession, sessionChatTitle, type ChatSessionFilter } from '../../session-chat/sessionChat'

  interface Props {
    sessions: SessionView[]
    selectedSessionId: string
    filter: ChatSessionFilter
    loading?: boolean
    onFilterChange: (filter: ChatSessionFilter) => void
    onSelect: (sessionId: string) => void
  }

  let { sessions, selectedSessionId, filter, loading = false, onFilterChange, onSelect }: Props = $props()

  const stateLabel = (session: SessionView) => isTerminalChatSession(session) ? 'Closed' : session.state
</script>

<div class="flex h-full flex-col rounded-xl border bg-card">
  <div class="border-b p-4">
    <div class="flex items-center justify-between gap-3">
      <div>
        <h3 class="font-semibold">Sessions</h3>
        <p class="text-sm text-muted-foreground">Pick an existing session to continue.</p>
      </div>
      <Badge variant="secondary">{sessions.length}</Badge>
    </div>
    <div class="mt-3 grid grid-cols-2 gap-2">
      <Button size="sm" variant={filter === 'active' ? 'default' : 'outline'} onclick={() => onFilterChange('active')}>Active</Button>
      <Button size="sm" variant={filter === 'all' ? 'default' : 'outline'} onclick={() => onFilterChange('all')}>All</Button>
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto p-3">
    {#if loading}
      <div class="space-y-2">
        <Skeleton class="h-20 w-full" />
        <Skeleton class="h-20 w-full" />
        <Skeleton class="h-20 w-full" />
      </div>
    {:else if !sessions.length}
      <Empty.Root class="py-10">
        <Empty.Header>
          <Empty.Media><MessageCircle class="size-6" /></Empty.Media>
          <Empty.Title>No sessions</Empty.Title>
          <Empty.Description>Create one from Session Console first.</Empty.Description>
        </Empty.Header>
      </Empty.Root>
    {:else}
      <div class="space-y-2">
        {#each sessions as session}
          <button
            type="button"
            class="w-full rounded-xl border p-3 text-left transition hover:bg-muted/70 {selectedSessionId === session.session_id ? 'border-primary bg-muted' : 'bg-background'}"
            onclick={() => onSelect(session.session_id)}
          >
            <div class="flex items-start justify-between gap-2">
              <span class="line-clamp-2 text-sm font-medium">{sessionChatTitle(session)}</span>
              <Badge variant="secondary" class="shrink-0">{stateLabel(session)}</Badge>
            </div>
            <p class="mt-1 truncate text-xs text-muted-foreground">{session.workspace_id ?? session.workspace ?? 'No workspace'}</p>
            <p class="mt-2 text-xs text-muted-foreground">Updated {formatDateTime(session.updated_at)}</p>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
