<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentClientView } from '../../api/types'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { selectTriggerClass } from '$lib/components/ui/select/index.js'
  import { cn } from '$lib/utils.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'

  let { id, client, workspaceId, value = $bindable(''), available = $bindable(false), allowDefault = false, disabled = false, class: className = 'w-full' }: {
    id: string
    class?: string
    client: Pick<AgentClientView, "name" | "client_type" | "client_key">
    workspaceId?: string
    value?: string
    available?: boolean
    allowDefault?: boolean
    disabled?: boolean
  } = $props()
  const device = useDeviceRuntime()
  let open = $state(false)
  const catalog = createQuery(
    () => ({
      queryKey: ['agent-client-models', client.client_type, client.client_key, workspaceId],
      queryFn: ({ signal }) => device.api.listAgentClientModels(client.client_type, client.client_key, signal, workspaceId),
      retry: false,
      refetchOnWindowFocus: false,
    }),
    () => device.queryClient,
  )
  let search = $state('')
  const defaultModel = $derived(catalog.data?.models.find((model) => model.id === catalog.data?.default_model))
  const current = $derived(value || (allowDefault ? catalog.data?.default_model : '') || '')
  const selected = $derived(catalog.data?.models.find((model) => model.id === current))
  const models = $derived(catalog.data?.models.filter((model) =>
    `${model.name} ${model.id}`.toLowerCase().includes(search.trim().toLowerCase()),
  ) ?? [])
  $effect(() => { if (!allowDefault && !value && defaultModel) value = defaultModel.id })
  $effect(() => { available = catalog.isSuccess && Boolean(selected) })

  function choose(id: string) {
    value = allowDefault && id === catalog.data?.default_model ? '' : id
    open = false
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Trigger {id} type="button" data-size="default" class={cn(selectTriggerClass, className)} {disabled}
    onclick={() => { search = '' }}>
    <span class="min-w-0 truncate text-left">{selected?.name ?? (current || (allowDefault ? $t('model.default') : $t('model.choose')))}</span>
    <CaretDownIcon class="size-4 shrink-0 text-muted-foreground" />
  </Dialog.Trigger>
  {#if !allowDefault}
    {#if catalog.isPending}<p role="status" class="text-sm text-muted-foreground">{$t('model.loading')}</p>
    {:else if catalog.isError}
      <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
      <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
    {:else if !catalog.data?.models.length}<p role="status" class="text-sm text-muted-foreground">{$t('model.empty')}</p>
    {:else if value && !selected}<p role="status" class="text-sm text-muted-foreground">{$t('bots.modelUnavailable')}</p>{/if}
  {/if}

  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{$t('model.choose')}</Dialog.Title>
      <Dialog.Description>{client.name}</Dialog.Description>
    </Dialog.Header>
    <Input aria-label={$t('model.search')} placeholder={$t('model.searchPlaceholder')} bind:value={search} />
    {#if allowDefault && !defaultModel}
      <Button type="button" variant="ghost" onclick={() => choose('')}>{$t('model.default')}</Button>
    {/if}
    {#if catalog.isPending}<p role="status">{$t('model.loading')}</p>
    {:else if catalog.isError}
      <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
      <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
    {:else}
      <ul aria-label={$t('model.available')} class="max-h-80 space-y-1 overflow-y-auto">
        {#each models as model (model.id)}
          <li>
            <Button type="button" variant="ghost" class="h-auto w-full justify-start gap-3 px-3 py-2 text-left"
              aria-label={model.name} onclick={() => choose(model.id)}>
              <span class="min-w-0 flex-1 whitespace-normal">
                <span class="block font-medium">{model.name}{#if model.id === defaultModel?.id}<span class="ml-2 text-xs font-normal text-muted-foreground">{$t('model.defaultTag')}</span>{/if}</span>
                <span class="block text-xs text-muted-foreground">{model.id}</span>
              </span>
              {#if model.id === current}<CheckIcon class="size-4 shrink-0" aria-label={$t('model.current')} />{/if}
            </Button>
          </li>
        {:else}<li class="py-3 text-sm text-muted-foreground">{$t('model.empty')}</li>{/each}
      </ul>
    {/if}
  </Dialog.Content>
</Dialog.Root>
