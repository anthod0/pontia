<script lang="ts">
  import { t } from '$lib/i18n'
  import * as Select from '$lib/components/ui/select/index.js'
  import type { ReasoningEffortOption } from '../../api/types'
  import { targetSegmentClass } from './targetSegment'

  let { options, defaultEffort, value = $bindable(''), disabled }: {
    options: ReasoningEffortOption[]
    defaultEffort?: string | null
    value?: string
    disabled: boolean
  } = $props()
  const selected = $derived(options.find((option) => option.id === value))
  const defaultName = $derived(options.find((option) => option.id === defaultEffort)?.name ?? defaultEffort)
  const defaultLabel = $derived(defaultName ? $t('effort.defaultValue', { value: defaultName }) : $t('effort.clientDefault'))
  $effect(() => { if (value && !selected) value = '' })
</script>

{#if options.length}
  <Select.Root type="single" bind:value {disabled}>
    <Select.Trigger class={targetSegmentClass} aria-label={$t('effort.title')}>
      <span class="min-w-0 truncate">{$t('effort.value', { value: selected?.name ?? defaultLabel })}</span>
    </Select.Trigger>
    <Select.Content align="start">
      <Select.Item value="" label={defaultLabel}>{defaultLabel}</Select.Item>
      {#each options as option (option.id)}
        <Select.Item value={option.id} label={option.name}>
          <span>{option.name}</span>
          {#if option.description}<span class="text-muted-foreground">{option.description}</span>{/if}
        </Select.Item>
      {/each}
    </Select.Content>
  </Select.Root>
{/if}
