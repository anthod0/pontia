<script lang="ts">
  import { onMount } from 'svelte'
  import { listAgentClients, patchAgentClient, resetAgentClient } from '../api/client'
  import type { AgentClientView, JsonObject } from '../api/types'
  import { customClientEntries } from '../clients'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'

  let clients = $state<AgentClientView[]>([])
  let selected = $state<AgentClientView | null>(null)
  let draft = $state('{}')
  let error = $state('')
  let notice = $state('')
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
    notice = ''
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
    notice = ''
    try { await action() } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  function save() {
    return perform(async () => {
      const overrides: unknown = JSON.parse(draft)
      if (!overrides || typeof overrides !== 'object' || Array.isArray(overrides)) throw new Error('Overrides must be a JSON object')
      const target = custom ? { client_type: custom.clientType, client_key: customKey.trim() } : selected
      if (!target || !target.client_key) throw new Error('Enter a stable client ID')
      const keyError = custom?.validateKey(target.client_key)
      if (keyError) throw new Error(keyError)
      if (custom && clients.some((client) => client.client_type === target.client_type && client.client_key === target.client_key)) throw new Error('This client already exists; select it to edit its settings')
      const saved = await patchAgentClient(target, { config_overrides: overrides as JsonObject })
      clients = await listAgentClients()
      select(saved)
      notice = 'Database overrides saved. Effective configuration may still be overridden by the service environment.'
    })
  }

  function toggle(client: AgentClientView) {
    return perform(async () => {
      const saved = await patchAgentClient(client, { enabled: !client.enabled })
      clients = clients.map((entry) => entry.id === saved.id ? saved : entry)
      selected = saved
      notice = 'Admission setting saved. Existing Sessions are unchanged.'
    })
  }

  function reset(client: AgentClientView) {
    resetOpen = false
    return perform(async () => {
      await resetAgentClient(client)
      await refresh()
      notice = 'Database settings removed. File and default configuration now apply.'
    })
  }
</script>

<div class="space-y-6">
  <div>
    <h1 class="text-xl font-semibold">Agent Clients</h1>
    <p class="mt-2 text-sm text-muted-foreground">Manage individual clients. Settings apply to new Sessions, not running instances. Enabled does not mean installed or connected.</p>
  </div>
  {#if error}<Alert.Root variant="destructive"><Alert.Description>{error}</Alert.Description></Alert.Root>{/if}
  {#if notice}<p role="status" class="text-sm">{notice}</p>{/if}
  <div class="flex flex-wrap gap-2">
    <Button variant="outline" disabled={busy || loading} onclick={() => perform(refresh)}>Refresh</Button>
    {#each customClientEntries as entry}
      <Button variant="outline" disabled={busy} onclick={() => { custom = entry; selected = null; customKey = ''; draft = '{}'; error = ''; notice = '' }}>{entry.label}</Button>
    {/each}
  </div>
  {#if loading}<p role="status">Loading clients…</p>{/if}
  <div class="grid gap-3 sm:grid-cols-2">
    {#each clients as client (client.id)}
      <Button variant={selected?.id === client.id ? 'secondary' : 'outline'} class="h-auto justify-between gap-2 p-4" disabled={busy} onclick={() => select(client)}>
        <span class="text-left"><span class="block">{client.name}</span><span class="block text-xs text-muted-foreground">{client.short_id}</span></span>
        <span class="text-xs">{client.enabled ? 'Enabled' : 'Disabled'} · {client.has_record ? 'Database' : 'Inherited'}</span>
      </Button>
    {/each}
  </div>
  {#if selected || custom}
    <Card.Root>
      <Card.Header><Card.Title>{selected?.name ?? 'Custom client'}</Card.Title></Card.Header>
      <Card.Content class="space-y-5">
        {#if selected}
          <p class="break-all text-xs text-muted-foreground">Integration: {selected.client_type} · CLI: {selected.short_id}<br />Unique ID: {selected.id}</p>
          <div class="flex flex-wrap gap-2">
            <Button variant="outline" disabled={busy} onclick={() => selected && toggle(selected)}>{selected.enabled ? 'Disable' : 'Enable'}</Button>
            <Button variant="outline" disabled={busy || !selected.has_record} onclick={() => resetOpen = true}>Restore inheritance</Button>
          </div>
        {:else if custom}
          <div class="space-y-2"><Label for="client-key">{custom.keyLabel}</Label><Input id="client-key" bind:value={customKey} disabled={busy} /><p class="text-xs text-muted-foreground">A stable identifier, not the display name. Configure the display name and launch source in the overrides below.</p></div>
        {/if}
        <form class="space-y-3" onsubmit={(event) => { event.preventDefault(); save() }}>
          <Label for="client-overrides">Database overrides (JSON)</Label>
          <Textarea id="client-overrides" bind:value={draft} rows={10} class="font-mono text-xs" disabled={busy} />
          <p class="text-xs text-muted-foreground">Saving replaces the entire database override object. Omit a field to inherit; null explicitly clears it. Environment variables have highest priority.</p>
          <Button type="submit" disabled={busy}>Save overrides</Button>
        </form>
        {#if selected}
          <div class="space-y-2"><h2 class="text-sm font-medium">Effective configuration</h2><pre class="overflow-auto rounded-md bg-muted p-3 text-xs">{JSON.stringify(selected.effective_config, null, 2)}</pre></div>
          <p class="text-xs text-muted-foreground">Database record: {selected.has_record ? 'present' : 'absent'} · Updated: {selected.updated_at ?? '—'}</p>
        {/if}
      </Card.Content>
    </Card.Root>
  {/if}
</div>

<AlertDialog.Root bind:open={resetOpen}>
  <AlertDialog.Content>
    <AlertDialog.Header><AlertDialog.Title>Restore inherited settings?</AlertDialog.Title><AlertDialog.Description>This removes the entire database record for {selected?.name}, including its enabled setting and overrides. It does not disable or uninstall the client. A database-only custom client will disappear from this list.</AlertDialog.Description></AlertDialog.Header>
    <AlertDialog.Footer><AlertDialog.Cancel>Cancel</AlertDialog.Cancel><AlertDialog.Action onclick={() => selected && reset(selected)}>Restore inheritance</AlertDialog.Action></AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
