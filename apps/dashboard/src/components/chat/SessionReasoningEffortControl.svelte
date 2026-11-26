<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { fromStore } from 'svelte/store'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import { creationOptionCatalog } from '../../queries/modelCatalog'
  import type { SessionView } from '../../api/types'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import GaugeIcon from 'phosphor-svelte/lib/GaugeIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import { meterBars, sessionCreationOption } from './modelEffort'

  let {
    session,
    busy = false,
    error = null,
    onChange,
  }: {
    session: SessionView
    busy?: boolean
    error?: string | null
    onChange: (effort: string) => void
  } = $props()

  const device = useDeviceRuntime()
  const creationOptions = fromStore(device.sessionCreation.createSessionCreationOptionsStore())
  const option = $derived(sessionCreationOption(session, creationOptions.current.data ?? []))
  const catalog = createQuery(
    () => creationOptionCatalog(device, option, session.workspace_id ?? ''),
    () => device.queryClient,
  )
  const model = $derived(catalog.data?.models.find((entry) => entry.id === session.model))
  const efforts = $derived(model?.reasoning_efforts ?? [])
  const current = $derived(
    efforts.find((effort) => effort.id === session.reasoning_effort)?.name ??
      session.reasoning_effort ??
      '—',
  )
  const running = $derived(['idle', 'busy', 'waiting_for_interaction'].includes(session.state))
  let open = $state(false)
</script>

<div class="shrink-0">
  <Popover.Root bind:open>
    <Popover.Trigger
      type="button"
      class="flex h-7 max-w-48 items-center gap-1.5 rounded-item px-1.5 font-mono text-11 text-muted-foreground outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default"
      aria-label={$t('effort.current', { value: current })}
      aria-busy={busy}
      disabled={!running || busy}
    >
      {#if busy}<SpinnerGapIcon class="size-[13px] shrink-0 animate-spin" aria-hidden="true" />
      {:else}<GaugeIcon class="size-[13px] shrink-0" aria-hidden="true" />{/if}
      <span class="truncate">{current}</span>
    </Popover.Trigger>
    <Popover.Content
      side="top"
      align="end"
      sideOffset={6}
      aria-label={$t('effort.change')}
      class="w-[min(18rem,calc(100vw-2rem))] p-1"
    >
      <div class="flex h-9 items-center gap-2 border-b px-2 text-xs text-muted-foreground">
        <GaugeIcon class="size-3.5" aria-hidden="true" />
        <span>{$t('effort.title')}</span>
        {#if session.model}<span class="ml-auto min-w-0 truncate font-mono text-heading">{session.model}</span>{/if}
      </div>
      {#if catalog.isPending}
        <p role="status" class="px-2 py-3 text-sm text-muted-foreground">{$t('model.loading')}</p>
      {:else if catalog.isError && !catalog.data}
        <div class="grid gap-2 p-2">
          <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
          <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
        </div>
      {:else if efforts.length}
        <div role="listbox" aria-label={$t('effort.title')} class="py-1">
          {#each efforts as effort, index (effort.id)}
            {@const selected = effort.id === session.reasoning_effort}
            <button
              type="button"
              role="option"
              aria-selected={selected}
              class={cn('flex w-full items-start gap-2.5 rounded-tooltip px-2 py-[7px] text-left outline-none hover:bg-muted focus-visible:bg-muted', selected && 'bg-selected')}
              disabled={selected || busy}
              onclick={() => {
                open = false
                onChange(effort.id)
              }}
            >
              <span aria-hidden="true" class="mt-1 inline-flex h-3 w-[22px] shrink-0 items-end gap-0.5">
                {#each meterBars(index, efforts.length) as lit, bar (bar)}
                  <i class={cn('w-[3px] rounded-item', lit ? (selected ? 'bg-primary' : 'bg-muted-foreground') : 'bg-border')} style:height={`${4 + bar * 2.6}px`}></i>
                {/each}
              </span>
              <span class="min-w-0 flex-1">
                <span class={cn('block font-mono text-13 font-medium', selected ? 'text-primary' : 'text-heading')}>{effort.name}</span>
                {#if effort.description}<span class="mt-px block text-11 leading-snug text-muted-foreground">{effort.description}</span>{/if}
              </span>
              {#if selected}<CheckIcon class="mt-[3px] size-3.5 shrink-0 text-primary" aria-hidden="true" />{/if}
            </button>
          {/each}
        </div>
      {:else}
        <p role="status" class="px-2 py-3 text-sm text-muted-foreground">{$t('effort.noOptions')}</p>
      {/if}
    </Popover.Content>
  </Popover.Root>
  {#if error}<p role="alert" class="mt-1 max-w-64 text-right text-xs text-destructive">{$t('effort.updateFailed')}: {error}</p>{/if}
</div>
