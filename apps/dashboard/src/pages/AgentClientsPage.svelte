<script lang="ts">
  import { t, currentLocale } from '$lib/i18n';
  import { useDeviceRuntime } from '../device/context';
  import { onMount } from 'svelte'
  import { toast } from 'svelte-sonner'
  import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import WrenchIcon from 'phosphor-svelte/lib/WrenchIcon'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import ProhibitIcon from 'phosphor-svelte/lib/ProhibitIcon'
  import ClientIcon from '../components/chat/ClientIcon.svelte'
  import { isTransientNetworkError } from '../api/client'
  import type { AgentClientView, AgentClientSetupOperation } from '../api/types'
  import { formatDateTime } from '../lib/format'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  const device = useDeviceRuntime();
  const { listAgentClients, patchAgentClient, resetAgentClient, setupAgentClient, getAgentClientSetup } = device.api;

  let clients = $state<AgentClientView[]>([])
  let selected = $state<AgentClientView | null>(null)
  let error = $state('')
  let saving = $state(false)
  let startingSetup = $state(false)
  let setupOperation = $state<AgentClientSetupOperation | null>(null)
  let setupConnectionError = $state('')
  let setupLoading = $state(true)
  let busy = $derived(saving || startingSetup || setupOperation?.status === 'running')
  let selectedSetup = $derived(setupOperation?.client_type === selected?.client_type && setupOperation?.client_key === selected?.client_key ? setupOperation : null)
  let loading = $state(true)
  let resetOpen = $state(false)

  function select(client: AgentClientView) {
    selected = client
    error = ''
  }

  async function refresh() {
    clients = await listAgentClients()
    if (selected) selected = clients.find((client) => client.id === selected?.id) ?? null
  }

  onMount(() => {
    refresh().catch((cause) => error = String(cause)).finally(() => loading = false)
    const controller = new AbortController()
    let timer: ReturnType<typeof setTimeout>
    async function pollSetup() {
      try {
        const operation = await getAgentClientSetup({ signal: controller.signal })
        if (controller.signal.aborted) return
        const completed = operation && operation.status !== 'running' &&
          (setupOperation?.id !== operation.id || setupOperation.status === 'running')
        setupOperation = operation
        setupConnectionError = ''
        if (completed) await refresh()
      } catch (cause) {
        if (!controller.signal.aborted) setupConnectionError = cause instanceof Error ? cause.message : String(cause)
      } finally {
        if (!controller.signal.aborted) {
          setupLoading = false
          timer = setTimeout(pollSetup, 1500)
        }
      }
    }
    void pollSetup()
    return () => { controller.abort(); clearTimeout(timer) }
  })

  async function setup(client: AgentClientView) {
    startingSetup = true
    error = ''
    try {
      setupOperation = await setupAgentClient(client)
      setupConnectionError = ''
      if (setupOperation.status !== 'running') await refresh()
    } catch (cause) {
      if (isTransientNetworkError(cause)) setupConnectionError = String(cause)
      else error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      startingSetup = false
    }
  }

  async function perform(action: () => Promise<void>) {
    saving = true
    error = ''
    try { await action() } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { saving = false }
  }

  function toggle(client: AgentClientView) {
    return perform(async () => {
      const saved = await patchAgentClient(client, { enabled: !client.enabled })
      clients = clients.map((entry) => entry.id === saved.id ? saved : entry)
      selected = saved
      toast.success(saved.enabled ? $t('clients.enabled') : $t('clients.disabled'), { description: $t('clients.sessionsUnchanged') })
    })
  }

  function reset(client: AgentClientView) {
    resetOpen = false
    return perform(async () => {
      await resetAgentClient(client)
      await refresh()
      toast.success($t('clients.restored'))
    })
  }
</script>

{#snippet restoreButton()}
  <Button type="button" variant="ghost" disabled={busy} onclick={() => resetOpen = true}><ArrowCounterClockwiseIcon class="size-4" /> {$t('clients.restoreInheritance')}</Button>
{/snippet}

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2">
    <Button variant="outline" disabled={busy || loading} onclick={() => perform(refresh)}><ArrowsClockwiseIcon class="size-4" /> {$t('common.refresh')}</Button>
  </div>

  {#if error}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Description>{error}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if loading}
    <div role="status" aria-label={$t('clients.loading')} class="grid gap-4 lg:grid-cols-[11rem_minmax(0,1fr)]"><Skeleton class="h-64 w-full" /><Skeleton class="h-96 w-full" /></div>
  {:else}
    <div class="grid items-start gap-4 lg:grid-cols-[11rem_minmax(0,1fr)]">
      <nav aria-label={$t('clients.title')} class="space-y-0.5">
        {#each clients as client (client.id)}
          <button
            type="button"
            class="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-60 {selected?.id === client.id ? 'bg-muted font-medium text-heading' : client.detection.status !== 'not_detected' && client.enabled ? 'text-foreground' : 'text-muted-foreground'}"
            aria-current={selected?.id === client.id ? 'true' : undefined}
            disabled={saving || startingSetup}
            onclick={() => select(client)}
          >
            <ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-4 {client.detection.status !== 'not_detected' && client.enabled ? 'text-foreground' : 'opacity-50'}" />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{client.name}</span>
              <span class="block text-xs font-normal text-muted-foreground">{$t(`clients.detection.${client.detection.status}`)}</span>
            </span>
            {#if !client.enabled}<ProhibitIcon class="size-3.5 shrink-0 text-muted-foreground" aria-label={$t('common.disabled')} />{/if}
          </button>
        {:else}
          <p class="px-3 py-2 text-sm text-muted-foreground">{$t('clients.empty')}</p>
        {/each}
      </nav>

      {#if selected}
        <div class="min-w-0 space-y-6">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0 space-y-1">
              <h3 class="truncate text-lg font-semibold text-heading">{selected.name}</h3>
              <p class="text-xs text-muted-foreground">
                {selected.client_type} / {selected.short_id} · {selected.has_record ? $t('clients.databaseUpdated', { date: formatDateTime(selected.updated_at, $currentLocale) }) : $t('clients.inherited')}
              </p>
              <p class="break-all font-mono text-xs text-muted-foreground" title={$t('clients.uniqueId')}>{selected.id}</p>
              <div role="status" class="space-y-1 pt-2 text-sm">
                <p class="flex items-center gap-1.5">
                  {#if selected.detection.status === 'detected'}<CheckCircleIcon class="size-4 text-success" />
                  {:else if selected.detection.status === 'not_detected'}<WarningCircleIcon class="size-4 text-muted-foreground" />{/if}
                  {$t(`clients.detection.${selected.detection.status}`)}
                </p>
                {#if selected.detection.detail}<p class="break-words text-xs text-muted-foreground">{selected.detection.detail}</p>{/if}
              </div>
            </div>
            <div class="flex shrink-0 items-center gap-2 pt-1">
              <Label for="client-enabled" class="text-sm text-muted-foreground">{$t('common.enabled')}</Label>
              <Switch id="client-enabled" checked={selected.enabled} disabled={busy} onCheckedChange={() => selected && toggle(selected)} />
            </div>
          </div>

          {#if selected.setup_command}
            <section class="space-y-4 rounded-lg border bg-card p-4" aria-label={$t('clients.setup')}>
              <div class="flex flex-wrap items-start justify-between gap-4">
                <div class="space-y-1.5">
                  <h4 class="flex items-center gap-2 text-sm font-medium"><WrenchIcon class="size-4 text-muted-foreground" /> {$t('clients.setup')}</h4>
                  <p class="text-sm text-muted-foreground">{$t('clients.setupDescription')}</p>
                  <code class="inline-block rounded bg-muted px-2 py-1 text-xs">{selected.setup_command}</code>
                </div>
                <Button disabled={busy || setupLoading || !!setupConnectionError} onclick={() => selected && setup(selected)}>
                  {#if startingSetup || selectedSetup?.status === 'running'}<CircleNotchIcon class="size-4 animate-spin" />{$t('clients.settingUp')}
                  {:else}<WrenchIcon class="size-4" />{$t('clients.runSetup')}{/if}
                </Button>
              </div>
              {#if selectedSetup}
                <div class="space-y-3 border-t pt-3">
                  <div role="status" class="flex items-center gap-2 text-sm">
                    {#if selectedSetup.status === 'running'}<CircleNotchIcon class="size-4 animate-spin text-muted-foreground" />{$t('clients.setupRunning')}
                    {:else if selectedSetup.status === 'succeeded'}<CheckCircleIcon class="size-4 text-success" />{$t('clients.setupSucceeded')}
                    {:else}<WarningCircleIcon class="size-4 text-destructive" />{$t('clients.setupFailed')}{/if}
                  </div>
                  {#if selectedSetup.error}<p class="text-sm text-destructive">{selectedSetup.error}</p>{/if}
                  {#if selectedSetup.output}<pre aria-label={$t('clients.setupOutput')} class="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-3 font-mono text-xs leading-5">{selectedSetup.output}</pre>{/if}
                </div>
              {/if}
              {#if setupConnectionError}
                <p role="status" class="text-sm text-muted-foreground">{$t('clients.setupReconnecting')} {setupConnectionError}</p>
              {/if}
            </section>
          {/if}

          {#if selected.has_record}{@render restoreButton()}{/if}
        </div>
      {:else}
        <p class="py-2 text-sm text-muted-foreground">{$t('clients.selectClient')}</p>
      {/if}
    </div>
  {/if}
</section>

<AlertDialog.Root bind:open={resetOpen}>
  <AlertDialog.Content>
    <AlertDialog.Header><AlertDialog.Title>{$t('clients.restoreConfirm')}</AlertDialog.Title><AlertDialog.Description>{$t('clients.restoreBefore')} {selected?.name}{$t('clients.restoreAfter')}</AlertDialog.Description></AlertDialog.Header>
    <AlertDialog.Footer><AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel><AlertDialog.Action onclick={() => selected && reset(selected)}>{$t('clients.restoreInheritance')}</AlertDialog.Action></AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
