<script lang="ts">
  import { onMount } from 'svelte'
  import { BriefcaseBusiness, CheckCircle2, CircleAlert, RadioTower } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { agentProfiles, agentProfilesError, agentProfilesLoading, loadAgentProfiles } from '../stores/agentProfiles'
  import { token } from '../stores/auth'
  import { lastConnectionError, sseStatus } from '../stores/connection'
  import { loadTasks, tasksError, tasksLoading } from '../stores/tasks'
  import { loadWorkspaces, workspaces, workspacesError, workspacesLoading } from '../stores/workspaces'

  onMount(() => {
    void Promise.all([loadTasks(), loadWorkspaces(), loadAgentProfiles()])
  })

  $: loading = $tasksLoading || $workspacesLoading || $agentProfilesLoading
  $: errors = [$tasksError, $workspacesError, $agentProfilesError].filter(Boolean)
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <h2 class="text-3xl font-semibold tracking-tight">Overview</h2>
      <p class="max-w-3xl text-muted-foreground">
        Summary data is loaded from External API responses and updated by the dashboard SSE stream.
      </p>
    </div>
    <Button variant="outline" onclick={() => void Promise.all([loadTasks(), loadWorkspaces(), loadAgentProfiles()])}>Refresh</Button>
  </div>

  {#if !$token.trim()}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>External API token required</Alert.Title>
      <Alert.Description>
        Set a bearer token in Settings to load dashboard data and open the live event stream.
        <Button variant="link" class="h-auto px-1" href="/dashboard/settings">Open Settings</Button>
      </Alert.Description>
    </Alert.Root>
  {/if}

  {#if errors.length}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Some dashboard data failed to load</Alert.Title>
      <Alert.Description>{errors.join(' · ')}</Alert.Description>
    </Alert.Root>
  {/if}

  <div class="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
    <Card.Root>
      <Card.Header class="flex flex-row items-center justify-between space-y-0">
        <div>
          <Card.Title>Workspaces</Card.Title>
          <Card.Description>Registered execution contexts</Card.Description>
        </div>
        <BriefcaseBusiness class="size-5 text-muted-foreground" />
      </Card.Header>
      <Card.Content class="text-3xl font-semibold">{#if loading}<Skeleton class="h-9 w-16" />{:else}{$workspaces.length}{/if}</Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header class="flex flex-row items-center justify-between space-y-0">
        <div>
          <Card.Title>Agent Profiles</Card.Title>
          <Card.Description>Available execution profiles</Card.Description>
        </div>
        <CheckCircle2 class="size-5 text-muted-foreground" />
      </Card.Header>
      <Card.Content class="text-3xl font-semibold">{#if loading}<Skeleton class="h-9 w-16" />{:else}{$agentProfiles.length}{/if}</Card.Content>
    </Card.Root>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title class="flex items-center gap-2"><RadioTower class="size-5" /> Live connection</Card.Title>
      <Card.Description>Dashboard-wide SSE stream from /external/v1/dashboard/events/stream.</Card.Description>
    </Card.Header>
    <Card.Content class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
      <div class="space-y-1">
        <div class="flex items-center gap-2">
          <Badge variant={$sseStatus === 'open' ? 'default' : 'secondary'}>{$sseStatus}</Badge>
          {#if $sseStatus === 'open'}<span class="text-sm text-muted-foreground">Receiving live updates</span>{/if}
        </div>
        {#if $lastConnectionError}<p class="text-sm text-destructive">{$lastConnectionError}</p>{/if}
      </div>

    </Card.Content>
  </Card.Root>
</section>
