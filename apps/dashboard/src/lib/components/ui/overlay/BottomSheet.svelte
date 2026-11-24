<script lang="ts">
  import type { ComponentProps } from 'svelte'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { cn } from '$lib/utils'
  import SheetHandle from './SheetHandle.svelte'

  let { dismiss, class: className, children, ...props }: ComponentProps<typeof Sheet.Content> & { dismiss: () => void } = $props()
</script>

<Sheet.Content {...props} side="bottom" class={cn('bottom-sheet gap-0 rounded-t-2xl px-4 pt-0', className)}>
  <SheetHandle {dismiss} />
  {@render children()}
</Sheet.Content>
