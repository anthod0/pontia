<script lang="ts">
  import { untrack } from 'svelte'
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { useDeviceRuntime } from '../../../device/context'
  import { creationOptionCatalog } from '../../../queries/modelCatalog'
  import { pickerRows, effortChoices, effortChipColumns } from '../../../components/chat/modelEffort'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import type { NewChatController } from '../../../features/chat/newChat.svelte'
  let { chat, close }: { chat: NewChatController; close: () => void } = $props()
  const device = useDeviceRuntime()
  let client = $state(untrack(() => chat.creationOptionId))
  let drafts = $state<Record<string, { model: string; effort: string }>>(
    untrack(() => ({ [client]: { model: chat.model, effort: chat.reasoningEffort } })),
  )
  let search = $state('')
  const option = $derived(chat.creationOptions.find((o) => o.id === client))
  const catalog = createQuery(
    () => creationOptionCatalog(device, option, chat.workspaceId),
    () => device.queryClient,
  )
  const models = $derived(catalog.data?.models ?? [])
  const defaultModel = $derived(models.find((m) => m.id === catalog.data?.default_model))
  const selection = $derived(drafts[client])
  const selected = $derived(
    models.find((m) => m.id === selection.model) ?? (!selection.model ? defaultModel : undefined),
  )
  const efforts = $derived(effortChoices(selected))
  function switchClient(id: string) {
    if (id === client) return
    drafts[id] ??= { model: '', effort: '' }
    client = id
    search = ''
  }
</script>

<Sheet.Root
  open
  onOpenChange={(open) => {
    if (!open) close()
  }}
>
  <BottomSheet dismiss={close} class="data-[side=bottom]:h-[min(80dvh,42rem)]" aria-describedby={undefined}>
    <Sheet.Title class="mb-3 pr-12 text-lg font-semibold">{$t('chat.clientModel')}</Sheet.Title>
    <div role="radiogroup" aria-label={$t('clients.agentClient')} class="flex shrink-0 gap-2 overflow-x-auto pb-3">
      {#each chat.creationOptions as o (o.id)}
        <button
          role="radio"
          aria-checked={client === o.id}
          class="grid min-w-24 min-h-16 shrink-0 justify-items-center gap-1 rounded-item border px-3 py-2 aria-checked:border-primary aria-checked:bg-selected"
          onclick={() => switchClient(o.id)}
        >
          <ClientIcon clientType={o.client_type} clientKey={o.client_key} class="size-6" />{o.name}
        </button>
      {/each}
    </div>
    {#if option?.model_catalog_key}
      <input
        type="search"
        class="rounded-item border px-3 py-2 text-base"
        bind:value={search}
        placeholder={$t('model.searchPlaceholder')}
        aria-label={$t('model.search')}
      />
      <div class="min-h-0 flex-1 overflow-auto">
        {#if catalog.isPending}<p role="status">{$t('model.loading')}</p>
        {:else if catalog.isError && !catalog.data}<p role="alert">{catalog.error.message}</p>
          <button onclick={() => catalog.refetch()}>{$t('common.retry')}</button>
        {:else}
          {#each pickerRows(models, defaultModel, true, search) as row (row.key)}
            <button
              aria-pressed={selection.model === row.key}
              aria-label={row.clientDefault ? $t('model.default') : row.model.name}
              class="mobile-row flex-col items-start! gap-0! aria-pressed:bg-selected"
              onclick={() => {
                if (selection.model !== row.key) drafts[client] = { model: row.key, effort: '' }
              }}
            >
              {row.clientDefault ? $t('model.default') : row.model.name}
              {#if !row.clientDefault && row.key === defaultModel?.id}<small>{$t('model.defaultTag')}</small
                >{/if}<small class="font-mono text-muted-foreground"
                >{row.clientDefault
                  ? row.model
                    ? $t('model.defaultCurrent', { name: row.model.name })
                    : ''
                  : row.model.id}</small
              >
            </button>
          {:else}<p role="status">{$t('model.empty')}</p>{/each}
        {/if}
      </div>
      {#if efforts.length}
        <div class="border-t py-2 text-sm">{$t('effort.title')}</div>
        <div
          role="radiogroup"
          aria-label={$t('effort.title')}
          class="grid gap-1"
          style:grid-template-columns={effortChipColumns(efforts.length)}
        >
          {#each efforts as effort (effort)}
            <button
              class="min-h-11 rounded-item bg-muted aria-checked:bg-primary aria-checked:text-primary-foreground"
              role="radio"
              aria-checked={selection.effort === effort}
              onclick={() => (selection.effort = effort)}
              >{selected?.reasoning_efforts?.find((e) => e.id === effort)?.name ?? $t('effort.default')}</button
            >
          {/each}
        </div>
      {/if}
    {:else}<div class="flex-1"></div>{/if}
    <button
      class="mt-3 min-h-11 rounded-item bg-primary text-primary-foreground"
      onclick={() => {
        chat.selectClient(client, selection.model, selection.effort)
        close()
      }}>{$t('common.done')}</button
    >
  </BottomSheet>
</Sheet.Root>
