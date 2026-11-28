<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import MinusIcon from 'phosphor-svelte/lib/MinusIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  let { action, label, path, onclick, disabled = false }: {
    action: 'stage' | 'unstage' | 'discard'; label: string; path?: string; disabled?: boolean; onclick: () => void
  } = $props()
  const Icon = $derived(action === 'stage' ? PlusIcon : action === 'unstage' ? MinusIcon : ArrowCounterClockwiseIcon)
</script>
<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <Button {...props} variant="ghost" size="icon-circle-xs" class="text-muted-foreground hover:bg-border hover:text-heading touch-target:min-h-0! touch-target:min-w-0! touch-target:h-8 touch-target:w-10" aria-label={path ? `${label}: ${path}` : label} {disabled} {onclick}><Icon class="size-3.5 touch-target:size-4" /></Button>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content>{label}</Tooltip.Content>
</Tooltip.Root>
