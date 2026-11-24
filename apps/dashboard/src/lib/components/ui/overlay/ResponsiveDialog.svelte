<script lang="ts">
  import type { ComponentProps } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte'
  import BottomSheet from './BottomSheet.svelte'
  import { cn } from '$lib/utils'

  let { mode, dismiss, class: className, children, ...props }: Pick<ComponentProps<typeof Dialog.Content>, 'class' | 'children' | 'showCloseButton' | 'aria-describedby'> & (
    { mode: 'sheet'; dismiss: () => void } | { mode: 'fullscreen'; dismiss?: never }
  ) = $props()
  const mobile = new IsMobile()
  const mobileStack = useMobileStack()
</script>

{#if (mobileStack || mobile.current) && dismiss}
  <BottomSheet {dismiss} {...props}>{@render children()}</BottomSheet>
{:else}
  <Dialog.Content {...props} class={cn(className, mobile.current && mode === 'fullscreen' && 'top-(--visual-viewport-top) left-0 flex h-(--visual-viewport-height) max-h-none w-full max-w-none translate-none flex-col rounded-none pt-[max(1rem,env(safe-area-inset-top))] pb-[max(1rem,env(safe-area-inset-bottom))] sm:max-w-none')}>
    {@render children()}
  </Dialog.Content>
{/if}
