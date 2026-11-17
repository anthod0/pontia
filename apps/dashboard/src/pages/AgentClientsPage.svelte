<script lang="ts">
  import { t, currentLocale } from '$lib/i18n';
  import { useDeviceRuntime } from '../device/context';
  import { onMount } from 'svelte'
  import { toast } from 'svelte-sonner'
  import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ProhibitIcon from 'phosphor-svelte/lib/ProhibitIcon'
  import ClientIcon from '../components/chat/ClientIcon.svelte'
  import type { AgentClientView, JsonObject } from '../api/types'
  import { customClientEntries } from '../clients'
  import { formatDateTime } from '../lib/format'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  const device = useDeviceRuntime();
  const { listAgentClients, patchAgentClient, resetAgentClient } = device.api;

  let clients = $state<AgentClientView[]>([])
  let selected = $state<AgentClientView | null>(null)
  let draft = $state('{}')
  let error = $state('')
  let busy = $state(false)
  let loading = $state(true)
  let resetOpen = $state(false)
  let custom = $state<(typeof customClientEntries)[number] | null>(null)
  let customKey = $state('')

  function select(client: AgentClientView) {
    selected = client
    custom = null
    draft = JSON.stringify(client.config_overrides, null, 2)
    error = ''
  }

  function startCustom(entry: (typeof customClientEntries)[number]) {
    custom = entry
    selected = null
    customKey = ''
    draft = '{}'
    error = ''
  }

  async function refresh() {
    clients = await listAgentClients()
    if (selected) {
      const current = clients.find((client) => client.id === selected?.id)
      selected = current ?? null
      draft = JSON.stringify(current?.config_overrides ?? {}, null, 2)
    }
  }

  onMount(() => {
    refresh().catch((cause) => error = String(cause)).finally(() => loading = false)
  })

  async function perform(action: () => Promise<void>) {
    busy = true
    error = ''
    try { await action() } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  function save() {
    return perform(async () => {
      const overrides: unknown = JSON.parse(draft)
      if (!overrides || typeof overrides !== 'object' || Array.isArray(overrides)) throw new Error($t('clients.objectRequired'))
      const target = custom ? { client_type: custom.clientType, client_key: customKey.trim() } : selected
      if (!target || !target.client_key) throw new Error($t('clients.idRequired'))
      const keyError = custom?.validateKey(target.client_key)
      if (keyError) throw new Error(keyError)
      if (custom && clients.some((client) => client.client_type === target.client_type && client.client_key === target.client_key)) throw new Error($t('clients.alreadyExists'))
      const saved = await patchAgentClient(target, { config_overrides: overrides as JsonObject })
      clients = await listAgentClients()
      select(saved)
      toast.success($t('clients.saved'), { description: $t('clients.environmentPriority') })
    })
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
      toast.success($t('clients.restored'), { description: $t('clients.defaultsApplied') })
    })
  }
</script>

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2">
    {#each customClientEntries as entry}
      <Button disabled={busy} onclick={() => startCustom(entry)}><PlusIcon class="size-4" /> {entry.label}</Button>
    {/each}
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
            class="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-60 {selected?.id === client.id ? 'bg-muted font-medium text-heading' : client.enabled ? 'text-foreground' : 'text-muted-foreground'}"
            aria-current={selected?.id === client.id ? 'true' : undefined}
            disabled={busy}
            onclick={() => select(client)}
          >
            <ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-4 {client.enabled ? 'text-foreground' : 'opacity-50'}" />
            <span class="min-w-0 flex-1 truncate">{client.name}</span>
            {#if !client.enabled}<ProhibitIcon class="size-3.5 shrink-0 text-muted-foreground" aria-label={$t('common.disabled')} />{/if}
          </button>
        {:else}
          <p class="px-3 py-2 text-sm text-muted-foreground">{$t('clients.empty')}</p>
        {/each}
      </nav>

      {#if selected || custom}
        <div class="min-w-0 space-y-6">
          {#if selected}
            <div class="flex items-start justify-between gap-4">
              <div class="min-w-0 space-y-1">
                <h3 class="truncate text-lg font-semibold text-heading">{selected.name}</h3>
                <p class="text-xs text-muted-foreground">
                  {selected.client_type} / {selected.short_id} · {selected.has_record ? $t('clients.databaseUpdated', { date: formatDateTime(selected.updated_at, $currentLocale) }) : $t('clients.inherited')}
                </p>
                <p class="break-all font-mono text-xs text-muted-foreground" title={$t('clients.uniqueId')}>{selected.id}</p>
              </div>
              <div class="flex shrink-0 items-center gap-2 pt-1">
                <Label for="client-enabled" class="text-sm text-muted-foreground">{$t('common.enabled')}</Label>
                <Switch id="client-enabled" checked={selected.enabled} disabled={busy} onCheckedChange={() => selected && toggle(selected)} />
              </div>
            </div>
          {:else if custom}
            <div class="space-y-1">
              <h3 class="text-lg font-semibold text-heading">{$t('clients.newCustom')}</h3>
              <p class="text-sm text-muted-foreground">{$t('clients.registerDescription')}</p>
            </div>
            <div class="max-w-md space-y-2">
              <Label for="client-key">{custom.keyLabel}</Label>
              <Input id="client-key" bind:value={customKey} disabled={busy} class="font-mono" />
              <p class="text-xs text-muted-foreground">{$t('clients.idDescription')}</p>
            </div>
          {/if}

          <div class="grid gap-6 {selected ? 'xl:grid-cols-2' : ''}">
            <form class="space-y-2" onsubmit={(event) => { event.preventDefault(); save() }}>
              <Label for="client-overrides">{$t('clients.overrides')}</Label>
              <Textarea id="client-overrides" bind:value={draft} rows={12} spellcheck={false} class="min-h-56 font-mono text-xs" disabled={busy} />
              <p class="text-xs text-muted-foreground">{$t('clients.overridesDescription')}</p>
              <div class="flex flex-wrap items-center gap-2 pt-2">
                <Button type="submit" disabled={busy}>{$t('clients.saveOverrides')}</Button>
                {#if selected?.has_record}
                  <Button variant="ghost" disabled={busy} onclick={() => resetOpen = true}><ArrowCounterClockwiseIcon class="size-4" /> {$t('clients.restoreInheritance')}</Button>
                {/if}
              </div>
            </form>
            {#if selected}
              <div class="space-y-2">
                <h4 class="text-sm leading-none font-medium">{$t('clients.effectiveConfiguration')}</h4>
                <pre class="max-h-[22rem] min-h-56 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5">{JSON.stringify(selected.effective_config, null, 2)}</pre>
                <p class="text-xs text-muted-foreground">{$t('clients.configurationDescription')}</p>
              </div>
            {/if}
          </div>
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
