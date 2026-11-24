<script lang="ts">
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { tick } from 'svelte'
  import { createQuery } from '@tanstack/svelte-query'
  import { modelCatalogQuery } from '../../queries/modelCatalog'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentClientView, SessionModel } from '../../api/types'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Kbd } from '$lib/components/ui/kbd/index.js'
  import { selectTriggerClass } from '$lib/components/ui/select/index.js'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte.js'
  import { adjacentValue } from '$lib/shortcuts/sessionNavigation'
  import { cn } from '$lib/utils.js'
  import ClientIcon from './ClientIcon.svelte'
  import { targetLabelClass, targetSegmentClass } from './targetSegment'
  import { effortChipColumns, effortChoices, meterBars, pickerRows, type PickerRow } from './modelEffort'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import GaugeIcon from 'phosphor-svelte/lib/GaugeIcon'
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon'
  import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon'
  import SparkleIcon from 'phosphor-svelte/lib/SparkleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'

  let {
    id,
    client,
    workspaceId,
    model = $bindable(''),
    effort = $bindable(''),
    savedEffort,
    available = $bindable(false),
    effortUnavailable = $bindable(false),
    allowDefault = false,
    variant = 'form',
    disabled = false,
  }: {
    id: string
    client: Pick<AgentClientView, 'name' | 'client_type' | 'client_key' | 'version'>
    workspaceId?: string
    model?: string
    effort?: string
    /** The persisted effort for the current model; it stays selected while it is unavailable. */
    savedEffort?: string | null
    available?: boolean
    effortUnavailable?: boolean
    allowDefault?: boolean
    variant?: 'toolbar' | 'form'
    disabled?: boolean
  } = $props()

  const uid = $props.id()
  const device = useDeviceRuntime()
  const isMobile = new IsMobile()
  const catalog = createQuery(
    () => modelCatalogQuery(device, client, workspaceId),
    () => device.queryClient,
  )

  let open = $state(false)
  let search = $state('')
  let highlight = $state<string | null>(null)
  let column = $state<'models' | 'efforts'>('models')
  let effortHighlight = $state('')

  const models = $derived(catalog.data?.models ?? [])
  const defaultModel = $derived(models.find((entry) => entry.id === catalog.data?.default_model))
  const current = $derived(model || (allowDefault ? catalog.data?.default_model : '') || '')
  const selected = $derived(models.find((entry) => entry.id === current))
  const selectedEfforts = $derived(selected?.reasoning_efforts ?? [])
  const savedUnavailable = $derived(Boolean(selected && effort && effort === savedEffort && !selectedEfforts.some((option) => option.id === effort)))
  const rows = $derived(pickerRows(models, defaultModel, allowDefault, search))
  const allRows = $derived(pickerRows(models, defaultModel, allowDefault, ''))
  const matchCount = $derived(rows.filter((row) => !row.clientDefault).length)
  const activeRow = $derived(rows.find((row) => row.key === highlight) ?? rows[0])
  const activeChoices = $derived(effortChoices(activeRow?.model))
  const activeId = $derived(column === 'efforts'
    ? `${uid}-effort-${activeChoices.indexOf(effortHighlight)}`
    : activeRow ? `${uid}-model-${rows.indexOf(activeRow)}` : undefined)
  const selectedRow = $derived(allRows.find((row) => row.key === model))
  const sheetModel = $derived(selectedRow?.model)
  const sheetChoices = $derived(effortChoices(sheetModel))
  const triggerName = $derived(selected?.name ?? (current || (allowDefault ? $t('model.default') : $t('model.choose'))))

  $effect(() => { if (!allowDefault && !model && defaultModel) model = defaultModel.id })
  $effect(() => { available = Boolean(selected) })
  $effect(() => { effortUnavailable = savedUnavailable })
  $effect(() => { if (selected && effort && !savedUnavailable && !selectedEfforts.some((option) => option.id === effort)) effort = '' })

  function optionName(entry: SessionModel | undefined, value: string): string {
    return entry?.reasoning_efforts?.find((option) => option.id === value)?.name ?? value
  }

  function rowName(row: PickerRow | undefined): string {
    return row?.model?.name ?? $t('model.default')
  }

  function openChange(next: boolean) {
    if (!next) return
    search = ''
    highlight = selectedRow?.key ?? allRows[0]?.key ?? null
    column = 'models'
    effortHighlight = effort
    void catalog.refetch({ cancelRefetch: false })
  }

  function chooseModel(row: PickerRow) {
    if (effortChoices(row.model).length) {
      highlight = row.key
      column = 'efforts'
      effortHighlight = row.key === model ? effort : ''
      return
    }
    model = row.key
    effort = ''
    open = false
  }

  function chooseEffort(row: PickerRow, value: string) {
    model = row.key
    effort = value
    open = false
  }

  function selectMobileModel(row: PickerRow) {
    if (row.key === model) return
    model = row.key
    effort = ''
  }

  async function revealActive() {
    await tick()
    if (activeId) document.getElementById(activeId)?.scrollIntoView({ block: 'nearest' })
  }

  function keydown(event: KeyboardEvent) {
    if (!activeRow) return
    const input = event.currentTarget as HTMLInputElement
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      const direction = event.key === 'ArrowDown' ? 1 : -1
      if (column === 'models') highlight = adjacentValue(rows.map((row) => row.key), activeRow.key, direction)
      else effortHighlight = adjacentValue(activeChoices, effortHighlight, direction) ?? ''
      void revealActive()
    } else if (event.key === 'ArrowRight' && column === 'models' && activeChoices.length && input.selectionStart === input.value.length) {
      column = 'efforts'
      effortHighlight = activeRow.key === model && activeChoices.includes(effort) ? effort : ''
    } else if (event.key === 'ArrowLeft' && column === 'efforts') {
      column = 'models'
    } else if (event.key === 'Enter') {
      if (column === 'models') chooseModel(activeRow)
      else chooseEffort(activeRow, effortHighlight)
    } else {
      return
    }
    event.preventDefault()
  }

  const tagClass = 'inline-flex h-[18px] shrink-0 items-center rounded-sm px-1.5 text-[10.5px] font-medium'
  const triggerClass = $derived(variant === 'toolbar'
    ? cn(selectTriggerClass, targetSegmentClass, 'md:max-w-56')
    : cn(selectTriggerClass, 'w-full gap-2'))
</script>

{#snippet triggerContent()}
  {#if variant === 'toolbar'}
    <span class={targetLabelClass}>{triggerName}</span>
    {#if selectedEfforts.length || savedUnavailable}
      <span class="min-w-0 wrap-anywhere font-normal text-muted-foreground md:shrink-0">· {#if effort}<span class="font-mono text-xs">{optionName(selected, effort)}</span>{:else}{$t('effort.default')}{/if}</span>
    {/if}
  {:else}
    <span class="min-w-0 flex-1 truncate text-left">{triggerName}</span>
    {#if selected}
      {#if effort}
        <span class={cn(tagClass, 'bg-selected font-mono text-primary')}>{optionName(selected, effort)}</span>
      {:else if selectedEfforts.length}
        <span class={cn(tagClass, 'bg-muted text-muted-foreground')}>{$t('effort.default')}{#if selected.default_reasoning_effort}&nbsp;·&nbsp;<span class="font-mono">{optionName(selected, selected.default_reasoning_effort)}</span>{/if}</span>
      {:else}
        <span class="shrink-0 text-xs text-muted-foreground">{$t('effort.unsupported')}</span>
      {/if}
    {/if}
  {/if}
  <CaretDownIcon class="size-4 shrink-0 text-muted-foreground" />
{/snippet}

{#snippet catalogState()}
  {#if catalog.isPending}<p role="status" class="px-2 py-3 text-sm text-muted-foreground">{$t('model.loading')}</p>
  {:else if catalog.isError && !catalog.data}
    <div class="grid gap-2 p-2">
      <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
      <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
    </div>
  {:else if !matchCount && !rows.length}<p role="status" class="px-2 py-3 text-sm text-muted-foreground">{$t('model.empty')}</p>{/if}
{/snippet}

{#snippet meter(index: number, count: number, active: boolean)}
  <span aria-hidden="true" class="mt-1 inline-flex h-3 w-[22px] shrink-0 items-end gap-0.5">
    {#each meterBars(index, count) as lit, bar (bar)}
      <i class={cn('w-[3px] rounded-[1px]', lit ? (active ? 'bg-primary' : 'bg-muted-foreground') : 'bg-border')} style:height={`${4 + bar * 2.6}px`}></i>
    {/each}
  </span>
{/snippet}

{#snippet desktopPanel()}
  <div class="flex min-h-0 flex-col border-r">
    <div class="flex h-10 shrink-0 items-center gap-2 border-b px-3 text-muted-foreground">
      <MagnifyingGlassIcon class="size-3.5 shrink-0" aria-hidden="true" />
      <input role="combobox" aria-expanded="true" aria-controls={`${uid}-models`} aria-activedescendant={activeId} aria-label={$t('model.search')}
        placeholder={$t('model.searchPlaceholder')} bind:value={search} onkeydown={keydown}
        class="h-full min-w-0 flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-muted-foreground" />
      <Kbd class="font-mono">{client.name}</Kbd>
    </div>
    <div role="listbox" id={`${uid}-models`} aria-label={$t('model.available')} class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-1">
      {@render catalogState()}
      {#if catalog.data}
        {#each rows as row, index (row.key)}
          {@const choices = effortChoices(row.model)}
          {@const isSelected = row.key === model && (row.clientDefault || Boolean(selected))}
          {#if !row.clientDefault && index === (allowDefault && !search ? 1 : 0)}
            {#if allowDefault && !search}<div class="mx-2 my-1 h-px bg-border"></div>{/if}
            {#if allowDefault || search}<div class="px-2 pt-2 pb-1 text-[11px] font-medium text-muted-foreground">{search ? $t('model.matches', { count: matchCount }) : $t('model.allModels')}</div>{/if}
          {/if}
          <!-- Options are navigated from the search combobox via aria-activedescendant. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div role="option" id={`${uid}-model-${index}`} aria-selected={isSelected} aria-label={row.clientDefault ? $t('model.default') : row.model.name} tabindex="-1"
            class={cn('flex min-h-10 cursor-default items-center gap-2 rounded-sm px-2 py-1.5', row.key === activeRow?.key && 'bg-muted')}
            onpointermove={() => { if (highlight !== row.key || column !== 'models') { highlight = row.key; column = 'models' } }}
            onpointerdown={(event) => event.preventDefault()} onclick={() => chooseModel(row)}>
            <span class="min-w-0 flex-1">
              <span class={cn('flex items-center gap-1.5 text-[13px] font-medium', isSelected ? 'text-primary' : 'text-heading')}>
                <span class="truncate">{row.clientDefault ? $t('model.default') : row.model.name}</span>
                {#if !row.clientDefault && row.model.id === defaultModel?.id}<span class={cn(tagClass, 'bg-muted text-muted-foreground')}>{$t('model.defaultTag')}</span>{/if}
              </span>
              {#if row.clientDefault}
                {#if row.model}<span class="block truncate text-[11px] text-muted-foreground">{$t('model.defaultCurrent', { name: row.model.name })}</span>{/if}
              {:else}
                <span class="block truncate font-mono text-[11px] text-muted-foreground">{row.model.id}</span>
              {/if}
            </span>
            {#if isSelected && (choices.length || effort)}
              <span class={cn(tagClass, 'font-mono', effort ? 'bg-selected text-primary' : 'bg-muted text-muted-foreground')}>{effort ? optionName(row.model, effort) : $t('effort.default')}</span>
            {/if}
            {#if isSelected}<CheckIcon class="size-3.5 shrink-0 text-primary" aria-hidden="true" />{/if}
            {#if choices.length}<CaretRightIcon class="size-3 shrink-0 text-muted-foreground" aria-hidden="true" />{:else}<span class="w-3 shrink-0"></span>{/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
  <div class="flex min-h-0 flex-col bg-surface">
    <div class="flex h-10 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
      <GaugeIcon class="size-3.5 shrink-0" aria-hidden="true" />
      <span class="shrink-0">{$t('effort.title')}</span>
      {#if activeRow}<span class="ml-auto min-w-0 truncate font-medium text-heading">{rowName(activeRow)}</span>{/if}
    </div>
    {#if activeRow && activeChoices.length}
      {@const activeSelected = activeRow.key === model}
      {@const activeModel = activeRow.model}
      <div role="listbox" aria-label={$t('effort.title')} class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-1">
        {#each activeChoices as choice, index (choice)}
          {@const option = activeModel?.reasoning_efforts?.find((entry) => entry.id === choice)}
          {@const isSelected = activeSelected && effort === choice}
          {#if index === 1}<div class="mx-2 my-1 h-px bg-border"></div>{/if}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div role="option" id={`${uid}-effort-${index}`} aria-selected={isSelected} aria-label={option?.name ?? $t('effort.default')} tabindex="-1"
            class={cn('flex cursor-default items-start gap-2.5 rounded-sm px-2 py-[7px]', isSelected ? 'bg-selected' : column === 'efforts' && effortHighlight === choice && 'bg-muted')}
            onpointermove={() => { if (effortHighlight !== choice || column !== 'efforts') { effortHighlight = choice; column = 'efforts' } }}
            onpointerdown={(event) => event.preventDefault()} onclick={() => chooseEffort(activeRow, choice)}>
            {#if option}
              {@render meter(index - 1, activeChoices.length - 1, isSelected)}
            {:else}
              <span class="mt-0.5 inline-flex w-[22px] shrink-0 justify-center text-muted-foreground"><SparkleIcon class="size-[13px]" aria-hidden="true" /></span>
            {/if}
            <span class="min-w-0 flex-1">
              <span class={cn('flex items-center gap-1.5 text-[13px] font-medium', option && 'font-mono', isSelected ? 'text-primary' : 'text-heading')}>
                {option?.name ?? $t('effort.default')}
                {#if option && option.id === activeModel?.default_reasoning_effort}<span class={cn(tagClass, 'bg-muted font-sans text-muted-foreground')}>{$t('effort.modelDefaultTag')}</span>{/if}
              </span>
              {#if option?.description}<span class="mt-px block text-[11.5px] leading-snug text-muted-foreground">{option.description}</span>
              {:else if !option}
                <span class="mt-px block text-[11.5px] leading-snug text-muted-foreground">
                  {#if activeModel?.default_reasoning_effort}{$t('effort.followModel', { value: optionName(activeModel, activeModel.default_reasoning_effort) })}{:else}{$t('effort.clientDefault')}{/if}
                </span>
              {/if}
            </span>
            {#if isSelected}<CheckIcon class="mt-[3px] size-3.5 shrink-0 text-primary" aria-hidden="true" />{/if}
          </div>
        {/each}
      </div>
    {:else if activeRow}
      <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
        <span class="grid size-9 place-items-center rounded-lg bg-muted text-muted-foreground"><GaugeIcon class="size-4" aria-hidden="true" /></span>
        <p class="text-[13px] font-medium text-heading">{$t('effort.unsupported')}</p>
        <p class="max-w-52 text-xs leading-normal text-muted-foreground">{$t('effort.unsupportedDescription', { name: rowName(activeRow) })}</p>
      </div>
    {:else}
      <div class="flex-1"></div>
    {/if}
    <div class="flex h-8 shrink-0 items-center gap-3 border-t px-3 text-[11px] text-muted-foreground">
      <span class="inline-flex items-center gap-1"><Kbd>↑↓</Kbd>{$t('model.title')}</span>
      <span class="inline-flex items-center gap-1"><Kbd>←→</Kbd>{$t('effort.short')}</span>
      <span class="inline-flex items-center gap-1"><Kbd>↵</Kbd>{$t('common.confirm')}</span>
      <span class="inline-flex items-center gap-1"><Kbd>Esc</Kbd>{$t('common.close')}</span>
    </div>
  </div>
{/snippet}

{#snippet mobileSheet()}
  <div class="shrink-0">
    <div class="flex items-center gap-2 pr-2 pb-2 pl-4">
      <ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-4 text-foreground" />
      <Sheet.Title class="min-w-0 flex-1 truncate text-base font-semibold text-heading">{$t('model.pickerTitle')}</Sheet.Title>
      <Sheet.Description class="sr-only">{client.name}</Sheet.Description>
      <Sheet.Close>
        {#snippet child({ props })}
          <Button {...props} variant="ghost" size="icon" class="size-11 text-muted-foreground [&_svg:not([class*=size-])]:size-5" aria-label={$t('common.close')}><XIcon /></Button>
        {/snippet}
      </Sheet.Close>
    </div>
  </div>
  <div class="mx-4 mb-2 flex h-10 shrink-0 items-center gap-2 rounded-md border px-3 text-muted-foreground focus-within:border-primary">
    <MagnifyingGlassIcon class="size-4 shrink-0" aria-hidden="true" />
    <input type="search" aria-label={$t('model.search')} placeholder={$t('model.searchPlaceholder')} bind:value={search}
      class="h-full min-w-0 flex-1 bg-transparent text-base text-foreground outline-none placeholder:text-muted-foreground" />
  </div>
  <div role="listbox" aria-label={$t('model.available')} class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2">
    {@render catalogState()}
    {#if catalog.data}
      {#each rows as row (row.key)}
        {@const isSelected = row.key === model && (row.clientDefault || Boolean(selected))}
        <div role="option" aria-selected={isSelected} aria-label={row.clientDefault ? $t('model.default') : row.model.name} tabindex="0"
          class={cn('flex min-h-14 cursor-default items-center gap-2 rounded-md px-3 py-2 outline-none focus-visible:bg-muted', isSelected && 'bg-selected')}
          onclick={() => selectMobileModel(row)} onkeydown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); selectMobileModel(row) } }}>
          <span class="min-w-0 flex-1">
            <span class={cn('flex items-center gap-1.5 text-[15px] font-medium', isSelected ? 'text-primary' : 'text-heading')}>
              <span class="truncate">{row.clientDefault ? $t('model.default') : row.model.name}</span>
              {#if !row.clientDefault && row.model.id === defaultModel?.id}<span class={cn(tagClass, 'bg-muted text-muted-foreground')}>{$t('model.defaultTag')}</span>{/if}
            </span>
            {#if row.clientDefault}
              {#if row.model}<span class="block truncate text-xs text-muted-foreground">{$t('model.defaultCurrent', { name: row.model.name })}</span>{/if}
            {:else}
              <span class="block truncate font-mono text-xs text-muted-foreground">{row.model.id}</span>
            {/if}
          </span>
          {#if !row.clientDefault && !effortChoices(row.model).length}<span class={cn(tagClass, 'text-muted-foreground')}>{$t('effort.none')}</span>{/if}
          {#if isSelected}<CheckIcon class="size-[18px] shrink-0 text-primary" aria-hidden="true" />{/if}
        </div>
        {#if row.clientDefault}<div class="mx-2 my-1 h-px bg-border"></div>{/if}
      {/each}
    {/if}
  </div>
  <div class="shrink-0 border-t bg-surface px-4 pt-3">
    <div class="mb-2 flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <GaugeIcon class="size-3.5 shrink-0" aria-hidden="true" />
      <span class="shrink-0">{$t('effort.title')} ·</span>
      <span class="min-w-0 truncate font-medium text-heading">{selectedRow ? rowName(selectedRow) : triggerName}</span>
    </div>
    {#if sheetChoices.length}
      <div role="radiogroup" aria-label={$t('effort.title')} class="grid gap-1.5" style:grid-template-columns={effortChipColumns(sheetChoices.length)}>
        {#each sheetChoices as choice (choice)}
          {@const option = sheetModel?.reasoning_efforts?.find((entry) => entry.id === choice)}
          <button type="button" role="radio" aria-checked={effort === choice}
            class={cn('grid h-11 min-w-0 place-items-center truncate rounded-md px-2.5 text-[13px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring', option && 'font-mono',
              effort === choice ? 'bg-primary-solid text-primary-foreground' : 'bg-muted text-heading')}
            onclick={() => effort = choice}>{option?.name ?? $t('effort.default')}</button>
        {/each}
      </div>
      <p class="mt-2 min-h-4 text-xs text-muted-foreground">
        {#if !effort}
          {#if sheetModel?.default_reasoning_effort}{$t('effort.followModel', { value: optionName(sheetModel, sheetModel.default_reasoning_effort) })}{:else}{$t('effort.clientDefault')}{/if}
        {:else}
          {@const option = sheetModel?.reasoning_efforts?.find((entry) => entry.id === effort)}
          {#if option?.description}{option.description}{:else if sheetModel?.default_reasoning_effort}{$t('effort.modelDefaultIs', { value: optionName(sheetModel, sheetModel.default_reasoning_effort) })}{/if}
        {/if}
      </p>
    {:else}
      <div class="flex h-10 items-center gap-2 rounded-md border border-dashed px-3 text-[13px] text-muted-foreground">
        <InfoIcon class="size-[15px] shrink-0" aria-hidden="true" />
        <span class="min-w-0 truncate">{$t('effort.unsupportedNamed', { name: selectedRow ? rowName(selectedRow) : triggerName })}</span>
      </div>
    {/if}
    <Sheet.Close>
      {#snippet child({ props })}
        <Button {...props} size="lg" class="mt-3 h-11 w-full">{$t('common.done')}</Button>
      {/snippet}
    </Sheet.Close>
  </div>
{/snippet}

{#if isMobile.current}
  <Sheet.Root bind:open onOpenChange={openChange}>
    <Sheet.Trigger {id} type="button" data-size="default" data-placeholder={selected || current ? undefined : ''} aria-invalid={savedUnavailable || undefined} class={triggerClass} {disabled}>
      {@render triggerContent()}
    </Sheet.Trigger>
    <BottomSheet dismiss={() => open = false} showCloseButton={false} class="data-[side=bottom]:h-(--visual-viewport-height) px-0">
      {@render mobileSheet()}
    </BottomSheet>
  </Sheet.Root>
{:else}
  <Popover.Root bind:open onOpenChange={openChange}>
    <Popover.Trigger {id} type="button" data-size="default" data-placeholder={selected || current ? undefined : ''} aria-invalid={savedUnavailable || undefined} class={triggerClass} {disabled}>
      {@render triggerContent()}
    </Popover.Trigger>
    <Popover.Content side={variant === 'toolbar' ? 'top' : 'bottom'} align="start" sideOffset={6} aria-label={$t('model.pickerTitle')}
      class="grid h-[min(23.25rem,var(--bits-popover-content-available-height))] w-[min(38.75rem,calc(100vw-2rem))] grid-cols-[minmax(0,18.5rem)_minmax(0,1fr)] gap-0 overflow-hidden p-0">
      {@render desktopPanel()}
    </Popover.Content>
  </Popover.Root>
{/if}

{#if variant === 'form'}
  {#if catalog.isPending}<p role="status" class="text-sm text-muted-foreground">{$t('model.loading')}</p>
  {:else if catalog.isError && !catalog.data}
    <p role="alert" class="text-sm text-destructive">{catalog.error.message}</p>
    <Button type="button" variant="outline" disabled={catalog.isFetching} onclick={() => void catalog.refetch()}>{$t('common.retry')}</Button>
  {:else if !catalog.data?.models.length}<p role="status" class="text-sm text-muted-foreground">{$t('model.empty')}</p>
  {:else if model && !selected}<p role="status" class="text-sm text-muted-foreground">{$t('bots.modelUnavailable')}</p>{/if}
  {#if savedUnavailable}<p role="alert" class="text-sm text-destructive">{$t('effort.unavailable')}</p>{/if}
{/if}
