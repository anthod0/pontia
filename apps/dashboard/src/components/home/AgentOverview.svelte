<script lang="ts">
  import { ArrowRight } from '@lucide/svelte'
  import { navigate } from '$lib/navigation'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import type { SessionView, WorkspaceView } from '../../api/types'
  import { sessionStateDotClass } from '$lib/sessionState'
  import { sessionDisplayTitle } from '../../pages/sessions/sessionList'
  import {
    activeAgentSessions,
    agentStateLabel,
    agentWorkspaceLabel,
    formatAgentUpdatedAt,
    workspacesWithActiveAgents,
  } from '../../pages/home/agentOverview'

  interface Props {
    sessions: SessionView[]
    workspaces: WorkspaceView[]
    loading?: boolean
    selectedWorkspaceId?: string | null
    onWorkspaceChange?: (workspaceId: string | null) => void
  }

  let {
    sessions,
    workspaces,
    loading = false,
    selectedWorkspaceId = null,
    onWorkspaceChange = () => {},
  }: Props = $props()

  const allWorkspacesValue = '__all_workspaces__'
  const overviewWorkspaces = $derived(workspacesWithActiveAgents(workspaces, sessions))
  const effectiveWorkspaceId = $derived(overviewWorkspaces.some((workspace) => workspace.workspace_id === selectedWorkspaceId) ? selectedWorkspaceId : null)
  const selectorValue = $derived(effectiveWorkspaceId ?? allWorkspacesValue)
  const selectedWorkspace = $derived(overviewWorkspaces.find((workspace) => workspace.workspace_id === effectiveWorkspaceId) ?? null)
  const activeAgents = $derived(activeAgentSessions(sessions, effectiveWorkspaceId))
  const visibleAgents = $derived(activeAgents.slice(0, 6))

  function openAgent(sessionId: string): void {
    void navigate(`/chat/${sessionId}`)
  }

  function workspaceTitle(workspace: WorkspaceView): string {
    return workspace.name?.trim() || workspace.display_path
  }

  function activeCountForWorkspace(workspaceId: string): number {
    return activeAgentSessions(sessions, workspaceId).length
  }

</script>

{#if (loading && !sessions.length) || visibleAgents.length}
  <section aria-label="Active agents" class="mx-auto w-full max-w-4xl space-y-4">
    <div class="flex justify-end">
      <Select.Root
        type="single"
        value={selectorValue}
        onValueChange={(value) => onWorkspaceChange(value === allWorkspacesValue ? null : value)}
      >
        <Select.Trigger class="max-w-56 shrink-0" aria-label="Overview workspace">
          <span class="truncate">
            {#if effectiveWorkspaceId}
              {selectedWorkspace ? workspaceTitle(selectedWorkspace) : 'Workspace'}
            {:else}
              All workspaces
            {/if}
          </span>
        </Select.Trigger>
        <Select.Content align="end">
          <Select.Item value={allWorkspacesValue} label="All workspaces">
            <span class="flex w-full min-w-48 items-center justify-between gap-6">
              <span>All workspaces</span>
              <span class="text-xs tabular-nums text-muted-foreground">{activeAgentSessions(sessions).length}</span>
            </span>
          </Select.Item>
          {#each overviewWorkspaces as workspace (workspace.workspace_id)}
            <Select.Item value={workspace.workspace_id} label={workspaceTitle(workspace)}>
              <span class="flex w-full min-w-48 items-center justify-between gap-6">
                <span class="min-w-0 truncate">{workspaceTitle(workspace)}</span>
                <span class="text-xs tabular-nums text-muted-foreground">{activeCountForWorkspace(workspace.workspace_id)}</span>
              </span>
            </Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>

    <Card.Root class="gap-0 bg-transparent py-0 ring-0">
      {#if loading && !sessions.length}
        <Card.Content class="space-y-3 p-4">
          <Skeleton class="h-14 w-full" />
          <Skeleton class="h-14 w-full" />
          <Skeleton class="h-14 w-full" />
        </Card.Content>
      {:else}
        <div class="divide-y">
          {#each visibleAgents as agent (agent.session_id)}
            <button
              type="button"
              class="group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
              onclick={() => openAgent(agent.session_id)}
              aria-label={`Open ${sessionDisplayTitle(agent)}, ${agentStateLabel(agent.state)}`}
            >
              <span class="min-w-0 flex-1">
                <span class="flex min-w-0 items-center gap-2">
                  <span class="truncate text-sm font-medium">{sessionDisplayTitle(agent)}</span>
                  <span
                    class={`size-2 shrink-0 rounded-full ${sessionStateDotClass(agent.state)}`}
                    aria-label={`${agentStateLabel(agent.state)} session`}
                    title={agentStateLabel(agent.state)}
                  ></span>
                </span>
                <span class="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                  <span class="truncate">{agentWorkspaceLabel(agent, workspaces)}</span>
                  <span aria-hidden="true">·</span>
                  <span class="shrink-0">{agent.client_type}</span>
                  {#if agent.model}
                    <span aria-hidden="true">·</span>
                    <span class="truncate">{agent.model}</span>
                  {/if}
                </span>
              </span>
              <span class="hidden shrink-0 text-xs text-muted-foreground sm:block" title={new Date(agent.updated_at).toLocaleString()}>
                Updated {formatAgentUpdatedAt(agent.updated_at)}
              </span>
              <ArrowRight class="size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5 group-hover:text-foreground" />
            </button>
          {/each}
        </div>
        {#if activeAgents.length > visibleAgents.length}
          <Card.Footer class="justify-center border-t px-4 py-2 text-xs text-muted-foreground">
            Showing 6 of {activeAgents.length} active agents
          </Card.Footer>
        {/if}
      {/if}
    </Card.Root>
  </section>
{/if}
