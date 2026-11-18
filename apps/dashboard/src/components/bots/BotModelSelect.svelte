<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { BotClient } from '../../api/types'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button, buttonVariants } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'

  let { id, client, value = $bindable(''), available = $bindable(false), disabled = false }: {
    id: string
    client: BotClient
    value?: string
    available?: boolean
    disabled?: boolean
  } = $props()
  const device = useDeviceRuntime()
  const catalog = createQuery(
    () => ({
      queryKey: ['agent-client-models', client.client_type, client.client_key],
      queryFn: ({ signal }) => device.api.listAgentClientModels(client.client_type, client.client_key, signal),
      retry: false,
    }),
    () => device.queryClient,
  )
  let open = $state(false)
  let search = $state('')
  const selected = $derived(catalog.data?.find((model) => model.id === value))
  const models = $derived(catalog.data?.filter((model) =>
    `${model.name} ${model.id}`.toLowerCase().includes(search.trim().toLowerCase()),
  ) ?? [])
  $effect(() => { available = catalog.isSuccess && Boolean(selected) })
</script>

<Dialog.Root bind:open>
  <Dialog.Trigger {id} type="button" class={buttonVariants({ variant: 'outline', class: 'w-full justify-between' })} {disabled}
    onclick={() => { search = '' }}>
    <span class="truncate">{selected?.name ?? (value || $t('model.choose'))}</span>
    <CaretDownIcon class="size-4 shrink-0" />
  </Dialog.Trigger>
  {#if catalog.isPending}<p role="status" class="text-sm text-muted-foreground">{$t('model.loading')}</p>
  {:else if catalog.isError}
    <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
    <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
  {:else if !catalog.data?.length}<p role="status" class="text-sm text-muted-foreground">{$t('model.empty')}</p>
  {:else if value && !selected}<p role="status" class="text-sm text-muted-foreground">{$t('bots.modelUnavailable')}</p>{/if}

  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{$t('model.choose')}</Dialog.Title>
      <Dialog.Description>{client.name}</Dialog.Description>
    </Dialog.Header>
    <Input aria-label={$t('model.search')} placeholder={$t('model.searchPlaceholder')} bind:value={search} />
    {#if catalog.isPending}<p role="status">{$t('model.loading')}</p>
    {:else if catalog.isError}
      <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
      <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
    {:else}
      <ul aria-label={$t('model.available')} class="max-h-80 space-y-1 overflow-y-auto">
        {#each models as model (model.id)}
          <li>
            <Button type="button" variant="ghost" class="h-auto w-full justify-start gap-3 px-3 py-2 text-left"
              aria-label={model.name} onclick={() => { value = model.id; open = false }}>
              <span class="min-w-0 flex-1 whitespace-normal">
                <span class="block font-medium">{model.name}</span>
                <span class="block text-xs text-muted-foreground">{model.id}</span>
              </span>
              {#if model.id === value}<CheckIcon class="size-4 shrink-0" aria-label={$t('model.current')} />{/if}
            </Button>
          </li>
        {:else}<li class="py-3 text-sm text-muted-foreground">{$t('model.empty')}</li>{/each}
      </ul>
    {/if}
  </Dialog.Content>
</Dialog.Root>
