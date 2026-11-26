<script lang="ts">
  import CornersOutIcon from 'phosphor-svelte/lib/CornersOutIcon'
  import CornersInIcon from 'phosphor-svelte/lib/CornersInIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { t } from '$lib/i18n'
  import { Dialog } from 'bits-ui'
  import { tick, type Snippet } from 'svelte'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte'

  let { children, open, close, ref = $bindable(null), ...props }: {
    'aria-label': string
    'data-session-panel'?: boolean
    'data-bot-chat-panel'?: boolean
    onkeydown?: (event: KeyboardEvent) => void
    tabindex?: number
    children: Snippet<[boolean, Snippet]>
    open: boolean
    close: () => void
    ref?: HTMLElement | null
  } = $props()
  const mobile = new IsMobile(1024)
  let dock = $state<HTMLDivElement>()
  let renderDock = $state(false)
  let fullscreen = $state(false)
  $effect(() => {
    if (!open || mobile.current) fullscreen = false
  })
  let opener: Element | null

  // The shell animates the occupied width for both the panel and the chat beside it.
  $effect(() => {
    if (mobile.current || !dock) return
    if (open) {
      renderDock = true
      return
    }
    const shell = dock.closest('.file-preview-layout')
    const closing = shell?.getAnimations?.().find(animation =>
      animation instanceof CSSTransition && animation.transitionProperty === '--file-preview-open')
    if (!closing) {
      renderDock = false
      return
    }
    let current = true
    const finish = () => { if (current) renderDock = false }
    closing.finished.then(finish, finish)
    return () => { current = false }
  })

  $effect.pre(() => {
    if (open) opener = document.activeElement
    else if (ref?.contains(document.activeElement)) void restoreFocus()
  })

  async function restoreFocus(event?: Event) {
    event?.preventDefault()
    await tick()
    if (opener instanceof HTMLElement && opener.isConnected && (document.activeElement === document.body || ref?.contains(document.activeElement)))
      opener.focus({ preventScroll: true })
  }
  const panelClass = 'flex flex-col border-l bg-surface outline-none pt-[env(safe-area-inset-top)]'
  $effect(() => {
    if (!mobile.current || !open || !ref) return
    const shell = document.querySelector<HTMLElement>('.desktop-shell')
    if (!shell) return
    shell.inert = true
    return () => { shell.inert = false }
  })
</script>

{#snippet fullscreenButton()}
  {#if !mobile.current}
    {@const label = $t(fullscreen ? 'nav.restoreSplitView' : 'nav.expandPanel')}
    <Button variant="ghost" size="icon-circle-sm" class="p-0 pointer-coarse:size-11" aria-label={label} title={label} onclick={() => fullscreen = !fullscreen}>
      {#if fullscreen}<CornersInIcon />{:else}<CornersOutIcon />{/if}
    </Button>
  {/if}
{/snippet}

{#if mobile.current}
  <Dialog.Root {open} onOpenChange={next => { if (!next) close() }}>
    <Dialog.Portal>
      <Dialog.Overlay class="fixed inset-0 z-(--layer-panel) bg-overlay motion-safe:[animation-duration:200ms] motion-safe:data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=closed]:pointer-events-none motion-safe:data-[state=closed]:animate-out data-[state=closed]:fade-out-0" />
      <Dialog.Content bind:ref {...props} inert={!open} onCloseAutoFocus={restoreFocus} aria-modal="true" aria-labelledby={undefined} aria-describedby={undefined}
        class={panelClass + ' fixed right-0 top-(--visual-viewport-top) z-(--layer-panel) h-(--visual-viewport-height) w-full pb-[env(safe-area-inset-bottom)] motion-safe:[animation-duration:200ms] motion-safe:data-[state=open]:animate-in data-[state=open]:slide-in-from-right motion-safe:data-[state=closed]:animate-out data-[state=closed]:slide-out-to-right'}>
        {@render children(false, fullscreenButton)}
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>
{:else}
  <div bind:this={dock} data-docked-panel-open={open || undefined} data-docked-panel-fullscreen={(open && fullscreen) || undefined} inert={!open} aria-hidden={!open}
    class="fixed inset-y-0 right-0 z-(--layer-panel) w-(--file-preview-width) overflow-hidden">
    {#if renderDock}
      <aside bind:this={ref} {...props} class={panelClass + ' relative h-full w-(--file-preview-target-width) shrink-0'}>
        {@render children(fullscreen, fullscreenButton)}
      </aside>
    {/if}
  </div>
{/if}
