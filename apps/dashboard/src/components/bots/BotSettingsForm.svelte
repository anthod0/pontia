<script lang="ts">
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import { createQuery } from '@tanstack/svelte-query'
  import { currentLocale, t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { Bot } from '../../api/types'
  import BotAvatarPicker from './BotAvatarPicker.svelte'
  import { randomAvatar } from './avatar'
  import ClientIcon from '../chat/ClientIcon.svelte'
  import ModelEffortPicker from '../chat/ModelEffortPicker.svelte'
  import ModelSelectPlaceholder from '../chat/ModelSelectPlaceholder.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import { useAgentClientOnboardingStatus } from '../../features/onboarding/context.svelte'

  let { bot, onboarding, oncancel, onsaved, saving = $bindable(false) }: {
    bot?: Bot
    onboarding?: 'desktop' | 'mobile'
    oncancel?: () => void
    onsaved: (bot: Bot) => void
    saving?: boolean
  } = $props()
  const id = $props.id()
  const device = useDeviceRuntime()
  const clientOnboarding = useAgentClientOnboardingStatus()
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
  let reasoningEffort = $derived(bot?.reasoning_effort ?? '')
  let effortUnavailable = $state(false)
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
    if (!selected || !name.trim() || !modelAvailable || effortUnavailable || saving) return
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
        reasoning_effort: reasoningEffort || null,
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
  {#if onboarding !== 'desktop'}
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
    <ResponsiveSelect value={clientId} onValueChange={(value) => {
      clientId = value
      model = ''
      reasoningEffort = ''
      effortUnavailable = false
      modelAvailable = false
    }} disabled={saving || clientOnboarding?.probing || !clients.data?.length}
      label={$t('clients.agentClient')} triggerClass="w-full" id={`${id}-client`}
      options={clients.data ?? []} keyOf={client => client.id} labelOf={client => client.name}>
      {#snippet trigger()}
        <span class="flex min-w-0 items-center gap-2">
          {#if selected}<ClientIcon clientType={selected.client_type} clientKey={selected.client_key} class="size-4 text-foreground" />{/if}
          <span class="truncate">{clientOnboarding?.probing ? $t('onboarding.clients.probingMobile') : selected?.name ?? $t(clients.isPending ? 'common.loading' : 'clients.choose')}</span>
        </span>
      {/snippet}
      {#snippet option(client)}
        <ClientIcon clientType={client.client_type} clientKey={client.client_key} class="size-4 text-foreground" />
        {client.name}
      {/snippet}
    </ResponsiveSelect>
    {#if clients.isError}<p class="text-sm text-destructive" role="alert">{clients.error.message}</p>
    {:else if clients.data?.length === 0}<p class="text-sm text-muted-foreground">{$t('bots.noClients')}</p>{/if}
  </div>
  <div class="grid gap-2">
    <Label for={`${id}-model`}>{$t('bots.model')}</Label>
    {#if selected}
      {#key clientId}
        <ModelEffortPicker id={`${id}-model`} client={selected} bind:model bind:effort={reasoningEffort} savedEffort={bot?.model === model ? bot.reasoning_effort : null}
          bind:available={modelAvailable} bind:effortUnavailable disabled={saving} />
      {/key}
    {:else}
      <ModelSelectPlaceholder id={`${id}-model`} label={$t('model.choose')} />
    {/if}
  </div>
  {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
  <div class={['flex justify-end gap-2 touch-target:[&_button]:min-h-11', !onboarding && 'max-md:sticky max-md:bottom-0 max-md:bg-popover max-md:py-1']}>
    {#if oncancel}<Button variant="outline" type="button" disabled={saving} onclick={oncancel}>{$t('common.cancel')}</Button>{/if}
    <Button class={onboarding === 'mobile' ? 'w-full' : undefined} type="submit" disabled={saving || !selected || !name.trim() || !modelAvailable || effortUnavailable}>
      {$t(bot ? 'common.save' : onboarding ? 'bots.startChat' : 'bots.create')}
    </Button>
  </div>
</form>
