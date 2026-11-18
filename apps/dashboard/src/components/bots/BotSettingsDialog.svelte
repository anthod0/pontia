<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { Bot } from '../../api/types'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'

  let { bot, onclose, onsaved }: { bot?: Bot; onclose: () => void; onsaved: (bot: Bot) => void } = $props()
  const device = useDeviceRuntime()
  const clients = createQuery(
    () => ({
      queryKey: ['bot-clients'],
      queryFn: ({ signal }) => device.api.listBotClients(signal),
    }),
    () => device.queryClient,
  )
  let name = $derived(bot?.name ?? '')
  let description = $derived(bot?.description ?? '')
  let model = $derived(bot?.model ?? '')
  let clientId = $state('')
  let saving = $state(false)
  let error = $state('')
  const selected = $derived(clients.data?.find((client) => client.id === clientId))
  const title = $derived($t(bot ? 'bots.edit' : 'bots.create'))

  $effect(() => {
    if (!clientId && clients.data) {
      clientId = (bot
        ? clients.data.find((client) => client.client_type === bot?.client_type && client.client_key === bot?.client_key)?.id
        : clients.data[0]?.id) ?? ''
    }
  })

  async function save(event: SubmitEvent) {
    event.preventDefault()
    if (!selected || !name.trim() || !model.trim() || saving) return
    saving = true
    error = ''
    try {
      const saved = await device.api.saveBot({
        name: name.trim(),
        description: description.trim() || null,
        client_type: selected.client_type,
        client_key: selected.client_key,
        model: model.trim(),
      }, bot?.bot_id)
      void device.queryClient.invalidateQueries({ queryKey: ['bots'] })
      onsaved(saved)
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      saving = false
    }
  }
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open && !saving) onclose() }}>
  <Dialog.Content class="max-w-md" aria-describedby={undefined}>
    <Dialog.Header><Dialog.Title>{title}</Dialog.Title></Dialog.Header>
    <form class="grid gap-4" onsubmit={save}>
      <div class="grid gap-2">
        <Label for="bot-name">{$t('bots.name')}</Label>
        <Input id="bot-name" bind:value={name} required />
      </div>
      <div class="grid gap-2">
        <Label for="bot-description">{$t('bots.description')}</Label>
        <Textarea id="bot-description" bind:value={description} rows={2} />
      </div>
      <div class="grid gap-2">
        <Label for="bot-client">{$t('clients.agentClient')}</Label>
        <Select.Root type="single" bind:value={clientId} disabled={!clients.data?.length}>
          <Select.Trigger id="bot-client" class="w-full">
            {selected?.name ?? $t(clients.isPending ? 'common.loading' : 'clients.choose')}
          </Select.Trigger>
          <Select.Content>
            {#each clients.data ?? [] as client (client.id)}
              <Select.Item value={client.id} label={client.name}>{client.name}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
        {#if clients.isError}<p class="text-sm text-destructive" role="alert">{clients.error.message}</p>
        {:else if clients.data?.length === 0}<p class="text-sm text-muted-foreground">{$t('bots.noClients')}</p>{/if}
      </div>
      <div class="grid gap-2">
        <Label for="bot-model">{$t('bots.model')}</Label>
        <Input id="bot-model" bind:value={model} required placeholder="provider/model" />
      </div>
      {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
      <Dialog.Footer>
        <Button variant="outline" type="button" disabled={saving} onclick={onclose}>{$t('common.cancel')}</Button>
        <Button type="submit" disabled={saving || !selected || !name.trim() || !model.trim()}>
          {$t(bot ? 'common.save' : 'bots.create')}
        </Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
