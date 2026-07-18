<script lang="ts">
  import { onMount } from 'svelte'
  import { CircleAlert, KeyRound, RadioTower } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { startEventStream, stopEventStream } from '../services/eventStream'
  import { token } from '../stores/auth'
  import { dashboardStreamCursor, lastConnectionError, reconnectCount, resetConnectionState, sseStatus } from '../stores/connection'

  let draftToken = ''
  let saved = false

  onMount(() => {
    draftToken = $token
  })

  function saveToken(): void {
    token.set(draftToken.trim())
    saved = true
    stopEventStream()
    startEventStream()
  }

  function clearToken(): void {
    draftToken = ''
    token.set('')
    stopEventStream()
    resetConnectionState()
  }

  function reconnect(): void {
    stopEventStream()
    startEventStream()
  }
</script>

<section class="space-y-6">
  <div class="space-y-2">
    <h2 class="text-3xl font-semibold tracking-tight">Common settings</h2>
    <p class="max-w-3xl text-muted-foreground">
      Configure External API authentication and inspect the dashboard live data connection.
    </p>
  </div>

  {#if !$token.trim()}
    <Alert.Root variant="destructive">
      <CircleAlert class="size-4" />
      <Alert.Title>Missing bearer token</Alert.Title>
      <Alert.Description>API requests and SSE updates need an External API token. Paste it below; it is stored only in this browser's localStorage.</Alert.Description>
    </Alert.Root>
  {/if}

  <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(22rem,0.75fr)]">
    <Card.Root>
      <Card.Header>
        <Card.Title class="flex items-center gap-2"><KeyRound class="size-5" /> External API token</Card.Title>
        <Card.Description>Sent as Authorization: Bearer &lt;token&gt; to /external/v1/*.</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-4">
        <div class="space-y-2">
          <Label for="api-token">Bearer token</Label>
          <Input id="api-token" type="password" placeholder="Paste External API token" bind:value={draftToken} autocomplete="off" />
          <p class="text-xs text-muted-foreground">Current status: {$token.trim() ? 'token saved' : 'no token saved'}.</p>
        </div>
        <div class="flex flex-wrap gap-2">
          <Button onclick={saveToken}>Save token</Button>
          <Button variant="outline" onclick={clearToken}>Clear</Button>
        </div>
        {#if saved}<p class="text-sm text-muted-foreground">Token saved. Data refreshes will use the new value.</p>{/if}
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title class="flex items-center gap-2"><RadioTower class="size-5" /> Live stream</Card.Title>
        <Card.Description>Dashboard SSE connection state.</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-3 text-sm">
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">Status</span>
          <Badge variant={$sseStatus === 'open' ? 'default' : 'secondary'}>{$sseStatus}</Badge>
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">Reconnect attempts</span>
          <span>{$reconnectCount}</span>
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">Last cursor</span>
          <span class="max-w-48 truncate" title={$dashboardStreamCursor ?? ''}>{$dashboardStreamCursor ?? '—'}</span>
        </div>
        {#if $lastConnectionError}<p class="rounded-md bg-destructive/10 p-2 text-destructive">{$lastConnectionError}</p>{/if}
        <div class="flex flex-wrap gap-2 pt-2">
          <Button variant="outline" onclick={reconnect} disabled={!$token.trim()}>Reconnect</Button>
          <Button variant="ghost" onclick={stopEventStream}>Disconnect</Button>
        </div>
      </Card.Content>
    </Card.Root>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>Dashboard source</Card.Title>
      <Card.Description>Use the SvelteKit static SPA build as the configured dashboard source.</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-2 text-sm text-muted-foreground">
      <p>Build this app with <code class="rounded bg-muted px-1 py-0.5">pnpm --dir=apps/dashboard run build</code>.</p>
      <p>Serve it with <code class="rounded bg-muted px-1 py-0.5">[dashboard].source = "apps/dashboard/dist"</code> or <code class="rounded bg-muted px-1 py-0.5">PONTIA_DASHBOARD_SOURCE=apps/dashboard/dist</code>.</p>
    </Card.Content>
  </Card.Root>
</section>
