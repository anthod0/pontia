<script lang="ts" generics="T">
  import type { Snippet } from 'svelte'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import * as Select from '$lib/components/ui/select'
  import * as Sheet from '$lib/components/ui/sheet'
  import { Input } from '$lib/components/ui/input'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte'
  import { cn } from '$lib/utils'
  import { t } from '$lib/i18n'
  import BottomSheet from './BottomSheet.svelte'

  let { value = $bindable(), onValueChange, options, keyOf, labelOf, disabledOption, groupOf, label, id, title,
    disabled = false, triggerClass, contentClass, itemClass, side, align, trigger, option }: {
    value?: string
    onValueChange?: (value: string) => void
    options: readonly T[]
    keyOf: (item: T) => string
    labelOf: (item: T) => string
    disabledOption?: (item: T) => boolean
    groupOf?: (item: T) => string
    label: string
    id?: string
    title?: string
    disabled?: boolean
    triggerClass?: string
    contentClass?: string
    itemClass?: string
    side?: 'top' | 'bottom'
    align?: 'start' | 'center' | 'end'
    trigger: Snippet
    option: Snippet<[T]>
  } = $props()
  const mobile = new IsMobile()
  let open = $state(false)
  let search = $state('')
  const filtered = $derived(options.filter(item => labelOf(item).toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())))

  function choose(item: T) {
    open = false
    const next = keyOf(item)
    if (value === next) return
    value = next
    onValueChange?.(next)
  }
</script>

{#if mobile.current}
  <Sheet.Root bind:open onOpenChange={() => search = ''}>
    <Sheet.Trigger {id} {disabled} {title} aria-label={label} data-size="default" class={cn(Select.selectTriggerClass, triggerClass, 'min-h-11')}>
      {@render trigger()}<CaretDownIcon class="size-4 text-muted-foreground" />
    </Sheet.Trigger>
    <BottomSheet dismiss={() => open = false}>
      <Sheet.Title class="flex min-h-11 items-center pr-12 text-base">{label}</Sheet.Title>
      {#if options.length > 8}
        <Input type="search" bind:value={search} aria-label={$t('common.searchOptions')} placeholder={$t('common.searchOptions')} class="mb-2 shrink-0" />
      {/if}
      <div role="group" aria-label={label} class="min-h-0 overflow-y-auto overscroll-contain">
        {#each filtered as item, index (keyOf(item))}
          {#if groupOf && groupOf(item) && (!index || groupOf(item) !== groupOf(filtered[index - 1]))}
            <div class="px-2 py-1 text-xs text-muted-foreground">{groupOf(item)}</div>
          {/if}
          <button type="button" aria-pressed={value === keyOf(item)} disabled={disabledOption?.(item)} onclick={() => choose(item)}
            class="flex min-h-12 w-full items-center gap-2 rounded-md px-2 text-left text-base hover:bg-muted focus-visible:bg-muted disabled:opacity-50">
            {@render option(item)}
            {#if value === keyOf(item)}<CheckIcon class="ml-auto size-5 shrink-0 text-primary" />{/if}
          </button>
        {:else}
          {#if search}<p role="status" class="p-2 text-muted-foreground">{$t('common.noMatchingOptions')}</p>{/if}
        {/each}
      </div>
    </BottomSheet>
  </Sheet.Root>
{:else}
  <Select.Root type="single" bind:value {onValueChange} {disabled}>
    <Select.Trigger {id} {title} aria-label={label} class={triggerClass}>{@render trigger()}</Select.Trigger>
    <Select.Content {align} {side} class={contentClass}>
      {#if groupOf}
        {#each new Set(options.map(groupOf)) as group (group)}
          <Select.Group>
            {#if group}<Select.GroupHeading>{group}</Select.GroupHeading>{/if}
            {@render desktopItems(options.filter(item => groupOf?.(item) === group))}
          </Select.Group>
        {/each}
      {:else}
        {@render desktopItems(options)}
      {/if}
    </Select.Content>
  </Select.Root>
{/if}

{#snippet desktopItems(items: readonly T[])}
  {#each items as item (keyOf(item))}
    <Select.Item value={keyOf(item)} label={labelOf(item)} disabled={disabledOption?.(item)} class={itemClass}>
      {@render option(item)}
    </Select.Item>
  {/each}
{/snippet}
