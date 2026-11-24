<script lang="ts">
  import type { ComponentProps, Snippet } from 'svelte'
  import { DropdownMenu as Menu } from 'bits-ui'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte'
  import { Button } from '$lib/components/ui/button'
  import { t } from '$lib/i18n'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import SheetHandle from './SheetHandle.svelte'

  let { open = $bindable(false), label, trigger, children, keepClearOf, onCloseAutoFocus, ...content }: Omit<ComponentProps<typeof DropdownMenu.Content>, 'children'> & {
    open?: boolean
    label: string
    keepClearOf?: HTMLElement | null
    trigger?: Snippet
    children: Snippet
  } = $props()
  const mobile = new IsMobile()
  let sheetStyle = $state<string>()
  $effect(() => {
    const anchor = keepClearOf
    if (!open || !mobile.current || !anchor) return
    const viewport = window.visualViewport
    function position() {
      const rect = anchor!.getBoundingClientRect()
      const top = viewport?.offsetTop ?? 0
      const bottom = top + (viewport?.height ?? window.innerHeight)
      const below = bottom - rect.bottom - 8
      const above = rect.top - top - 8
      // Prefer the bottom edge. Flip above a low trigger when less than two rows fit below.
      const flip = below < 160 && above > below
      sheetStyle = `--menu-room: ${Math.max(0, flip ? above : below)}px; bottom: ${window.innerHeight - (flip ? rect.top - 8 : bottom)}px`
    }
    position()
    viewport?.addEventListener('resize', position)
    viewport?.addEventListener('scroll', position)
    return () => {
      viewport?.removeEventListener('resize', position)
      viewport?.removeEventListener('scroll', position)
    }
  })
</script>

<DropdownMenu.Root bind:open>
  {@render trigger?.()}
  {#if mobile.current}
    <DropdownMenu.Portal>
      {#if open}<div aria-hidden="true" class="fixed inset-0 z-(--layer-menu) bg-overlay"></div>{/if}
      <Menu.ContentStatic {onCloseAutoFocus} preventScroll aria-label={label} style={keepClearOf ? sheetStyle : undefined}
        class="bottom-sheet fixed inset-x-0 z-(--layer-menu) rounded-t-2xl border-t bg-popover px-3 text-popover-foreground outline-none [&_[role^=menuitem]]:min-h-12 [&_[role^=menuitem]]:text-base">
        <SheetHandle dismiss={() => open = false} />
        <div class="flex items-center justify-between gap-2 px-1">
          <span class="truncate font-semibold">{label}</span>
          <Button type="button" variant="ghost" size="icon" class="size-11 shrink-0" aria-label={$t('common.close')} onclick={() => open = false}><XIcon /></Button>
        </div>
        {@render children()}
      </Menu.ContentStatic>
    </DropdownMenu.Portal>
  {:else}
    <DropdownMenu.Content aria-label={label} {...content} {onCloseAutoFocus}>{@render children()}</DropdownMenu.Content>
  {/if}
</DropdownMenu.Root>
