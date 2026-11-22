<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { currentLocale, t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { Bot } from '../../api/types'
  import BotAvatarPicker from './BotAvatarPicker.svelte'
  import { randomAvatar } from './avatar'
  import ClientIcon from '../chat/ClientIcon.svelte'
  import ClientModelSelect from '../chat/ClientModelSelect.svelte'
  import ModelSelectPlaceholder from '../chat/ModelSelectPlaceholder.svelte'
  import * as Select from '$lib/components/ui/select/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'

  let { bot, onboarding = false, oncancel, onsaved, saving = $bindable(false) }: {
    bot?: Bot
    onboarding?: boolean
    oncancel?: () => void
    onsaved: (bot: Bot) => void
    saving?: boolean
  } = $props()
  const id = $props.id()
  const device = useDeviceRuntime()
  const clients = createQuery(
    () => ({
      queryKey: ['bot-clients'],
      queryFn: ({ signal }) => device.api.listBotClients(signal),
    }),
    () => device.queryClient,
  )
  let name = $derived(bot?.name ?? (onboarding ? 'Pontia' : ''))
  let description = $derived(bot?.description ?? '')
  const initialAvatar = randomAvatar()
  let avatarShape = $derived(bot?.avatar_shape ?? initialAvatar.shape)
  let avatarColor = $derived(bot?.avatar_color ?? initialAvatar.color)
  let model = $derived(bot?.model ?? '')
  let modelAvailable = $state(false)
  let clientId = $state('')
  let error = $state('')
  const selected = $derived(clients.data?.find((client) => client.id === clientId))

  $effect(() => {
    if (!clientId && clients.data) {
      clientId = (bot
        ? clients.data.find((client) => client.client_type === bot?.client_type && client.client_key === bot?.client_key)?.id
        : clients.data[0]?.id) ?? ''
    }
  })

  async function save(event: SubmitEvent) {
    event.preventDefault()
    if (!selected || !name.trim() || !modelAvailable || saving) return
    saving = true
    error = ''
    try {
      const input = {
        name: name.trim(),
        description: description.trim() || null,
        avatar_shape: avatarShape,
        avatar_color: avatarColor,
        client_type: selected.client_type,
        client_key: selected.client_key,
        model: model.trim(),
      }
      const saved = bot
        ? await device.api.saveBot(input, bot.bot_id)
        : await device.api.saveBot(input, undefined, $currentLocale)
      void device.queryClient.invalidateQueries({ queryKey: ['bots'] })
      onsaved(saved)
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      saving = false
    }
  }
</script>

<form class="grid gap-4" onsubmit={save}>
  <BotAvatarPicker bind:shape={avatarShape} bind:color={avatarColor} disabled={saving} />
  {#if !onboarding}
    <div class="grid gap-2">
      <Label for={`${id}-name`}>{$t('bots.name')}</Label>
      <Input id={`${id}-name`} bind:value={name} required />
    </div>
    {#if bot}
      <div class="grid gap-2">
        <Label for={`${id}-description`}>{$t('bots.description')}</Label>
        <Textarea id={`${id}-description`} bind:value={description} rows={2} />
      </div>
    {/if}
  {/if}
  <div class="grid gap-2">
    <Label for={`${id}-client`}>{$t('clients.agentClient')}</Label>
    <Select.Root type="single" value={clientId} onValueChange={(value) => {
      clientId = value
      model = ''
      modelAvailable = false
    }} disabled={saving || !clients.data?.length}>
      <Select.Trigger id={`${id}-client`} class="w-full">
        <span class="flex min-w-0 items-center gap-2">
          {#if selected}<ClientIcon clientType={selected.client_type} acpAgentId={selected.client_key} class="size-4 text-foreground" />{/if}
          <span class="truncate">{selected?.name ?? $t(clients.isPending ? 'common.loading' : 'clients.choose')}</span>
        </span>
      </Select.Trigger>
      <Select.Content>
        {#each clients.data ?? [] as client (client.id)}
          <Select.Item value={client.id} label={client.name}>
            <ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-4 text-foreground" />
            {client.name}
          </Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
    {#if clients.isError}<p class="text-sm text-destructive" role="alert">{clients.error.message}</p>
    {:else if clients.data?.length === 0}<p class="text-sm text-muted-foreground">{$t('bots.noClients')}</p>{/if}
  </div>
  <div class="grid gap-2">
    <Label for={`${id}-model`}>{$t('bots.model')}</Label>
    {#if selected}
      {#key clientId}
        <ClientModelSelect id={`${id}-model`} client={selected} bind:value={model} bind:available={modelAvailable} disabled={saving} />
      {/key}
    {:else}
      <ModelSelectPlaceholder id={`${id}-model`} label={$t('model.choose')} />
    {/if}
  </div>
  {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
  <div class="flex justify-end gap-2">
    {#if oncancel}<Button variant="outline" type="button" disabled={saving} onclick={oncancel}>{$t('common.cancel')}</Button>{/if}
    <Button type="submit" disabled={saving || !selected || !name.trim() || !modelAvailable}>
      {$t(bot ? 'common.save' : onboarding ? 'bots.startChat' : 'bots.create')}
    </Button>
  </div>
</form>
