<script lang="ts">
  import { selectTriggerClass } from '$lib/components/ui/select/index.js'
  import { cn } from '$lib/utils.js'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'

  // Stands in for ClientModelSelect until a client is chosen.
  let { id, label }: { id: string; label: string } = $props()
</script>

<button {id} type="button" data-size="default" data-placeholder class={cn(selectTriggerClass, 'w-full')} disabled>
  <span class="min-w-0 truncate">{label}</span>
  <CaretDownIcon class="size-4 shrink-0 text-muted-foreground" />
</button>
