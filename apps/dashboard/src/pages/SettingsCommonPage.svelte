<script lang="ts">
  import { statusLabel } from '$lib/i18n/status';
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../device/context';
  import { onMount } from 'svelte'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import BroadcastIcon from 'phosphor-svelte/lib/BroadcastIcon'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { token } from '$dashboard-mode/auth'
  import AppearanceSettings from '../components/settings/AppearanceSettings.svelte'
  const device = useDeviceRuntime();
  const { startEventStream, stopEventStream } = device.events;
  const {
    dashboardStreamCursor,
    lastConnectionError,
    reconnectCount,
    resetConnectionState,
    sseStatus,
  } = device.connection;

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
  <AppearanceSettings />

  {#if !$token.trim()}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>{$t('auth.missingToken')}</Alert.Title>
      <Alert.Description>{$t('auth.tokenDescription')}</Alert.Description>
    </Alert.Root>
  {/if}

  <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(22rem,0.75fr)]">
    <Card.Root>
      <Card.Header>
        <Card.Title class="flex items-center gap-2"><KeyIcon class="size-5" /> {$t('auth.externalToken')}</Card.Title>
        <Card.Description>{$t('auth.sentAs')}</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-4">
        <div class="space-y-2">
          <Label for="api-token">{$t('auth.bearerToken')}</Label>
          <Input id="api-token" type="password" placeholder={$t('auth.pasteToken')} bind:value={draftToken} autocomplete="off" />
          <p class="text-xs text-muted-foreground">{$t('settings.tokenStatus', { status: $token.trim() ? $t('auth.tokenSaved') : $t('auth.noToken') })}</p>
        </div>
        <div class="flex flex-wrap gap-2">
          <Button onclick={saveToken}>{$t('auth.saveToken')}</Button>
          <Button variant="outline" onclick={clearToken}>{$t('common.clear')}</Button>
        </div>
        {#if saved}<p class="text-sm text-muted-foreground">{$t('auth.savedDescription')}</p>{/if}
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title class="flex items-center gap-2"><BroadcastIcon class="size-5" /> {$t('connection.liveStream')}</Card.Title>
        <Card.Description>{$t('connection.streamDescription')}</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-3 text-sm">
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">{$t('common.status')}</span>
          <Badge variant={$sseStatus === 'open' ? 'default' : 'secondary'}>{statusLabel($sseStatus, $t)}</Badge>
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">{$t('connection.attempts')}</span>
          <span>{$reconnectCount}</span>
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="text-muted-foreground">{$t('connection.cursor')}</span>
          <span class="max-w-48 truncate" title={$dashboardStreamCursor ?? ''}>{$dashboardStreamCursor ?? '—'}</span>
        </div>
        {#if $lastConnectionError}<p class="rounded-md bg-destructive/10 p-2 text-destructive">{$lastConnectionError}</p>{/if}
        <div class="flex flex-wrap gap-2 pt-2">
          <Button variant="outline" onclick={reconnect} disabled={!$token.trim()}>{$t('connection.reconnect')}</Button>
          <Button variant="ghost" onclick={stopEventStream}>{$t('connection.disconnect')}</Button>
        </div>
      </Card.Content>
    </Card.Root>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>{$t('settings.source')}</Card.Title>
      <Card.Description>{$t('settings.sourceDescription')}</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-2 text-sm text-muted-foreground">
      <p>{$t('settings.buildWith')} <code class="rounded-sm bg-muted px-1 py-0.5">bun run --cwd apps/dashboard build</code>.</p>
      <p>{$t('settings.serveWith')} <code class="rounded-sm bg-muted px-1 py-0.5">[dashboard].source = "apps/dashboard/dist"</code> {$t('common.or')} <code class="rounded-sm bg-muted px-1 py-0.5">PONTIA_DASHBOARD_SOURCE=apps/dashboard/dist</code>.</p>
    </Card.Content>
  </Card.Root>
</section>
