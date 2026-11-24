<script lang="ts">
  import { Dialog } from 'bits-ui'
  import { tick, type Snippet } from 'svelte'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte'

  let { children, close, ref = $bindable(null), ...props }: {
    'aria-label': string
    'data-session-panel'?: boolean
    'data-bot-chat-panel'?: boolean
    onkeydown?: (event: KeyboardEvent) => void
    tabindex?: number
    children: Snippet
    close: () => void
    ref?: HTMLElement | null
  } = $props()
  const mobile = new IsMobile(1024)
  const opener = globalThis.document?.activeElement

  async function restoreFocus(event: Event) {
    event.preventDefault()
    await tick()
    if (opener instanceof HTMLElement && opener.isConnected && document.activeElement === document.body)
      opener.focus({ preventScroll: true })
  }
  const panelClass = 'fixed right-0 z-(--layer-panel) flex min-w-0 flex-col border-l bg-surface outline-none pt-[env(safe-area-inset-top)]'
  $effect(() => {
    if (!mobile.current || !ref) return
    const shell = document.querySelector<HTMLElement>('.file-preview-layout')
    if (!shell) return
    shell.inert = true
    return () => { shell.inert = false }
  })
</script>

{#if mobile.current}
  <Dialog.Root open onOpenChange={open => { if (!open) close() }}>
    <Dialog.Portal>
      <Dialog.Content bind:ref {...props} onCloseAutoFocus={restoreFocus} aria-modal="true" aria-labelledby={undefined} aria-describedby={undefined}
        class={panelClass + ' top-(--visual-viewport-top) h-(--visual-viewport-height) w-full pb-[env(safe-area-inset-bottom)]'}>
        {@render children()}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>
{:else}
  <aside bind:this={ref} {...props} class={panelClass + ' inset-y-0 w-[var(--file-preview-width)]'}>
    {@render children()}
  </aside>
{/if}
