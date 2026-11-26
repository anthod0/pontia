<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import { Toaster as Sonner, type ToasterProps } from 'svelte-sonner'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon'
  import WarningIcon from 'phosphor-svelte/lib/WarningIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { buttonVariants } from '$lib/components/ui/button/button.svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { resolvedTheme } from '$lib/theme'

  let { ...restProps }: ToasterProps = $props()
  let root: HTMLDivElement

  function setExpanded(toaster: HTMLOListElement, expanded: boolean, event: PointerEvent): void {
    toaster.dispatchEvent(new MouseEvent(expanded ? 'mouseenter' : 'mouseleave', {
      clientX: event.clientX,
      clientY: event.clientY,
    }))
  }

  onMount(() => {
    let pointer: { x: number; y: number; toaster: HTMLOListElement } | undefined

    const updateRoles = () => {
      for (const toast of root.querySelectorAll<HTMLElement>('[data-sonner-toast]')) {
        toast.setAttribute('role', toast.dataset.type === 'error' ? 'alert' : 'status')
      }
    }
    const observer = new MutationObserver(updateRoles)
    observer.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-type'] })
    updateRoles()

    const pointerDown = (event: PointerEvent) => {
      if (!isMobileShell() || !(event.target instanceof Element) || event.target.closest('button')) return
      const toaster = event.target.closest<HTMLOListElement>('[data-sonner-toaster]')
      if (toaster) pointer = { x: event.clientX, y: event.clientY, toaster }
    }
    const pointerUp = (event: PointerEvent) => {
      if (!pointer) return
      const { x, y, toaster } = pointer
      pointer = undefined
      if (Math.hypot(event.clientX - x, event.clientY - y) >= 5) return
      const expanded = toaster.querySelector('[data-sonner-toast][data-expanded="true"]') !== null
      queueMicrotask(() => setExpanded(toaster, !expanded, event))
    }
    const outsidePointerDown = (event: PointerEvent) => {
      if (!isMobileShell() || root.contains(event.target as Node)) return
      const toaster = root.querySelector<HTMLOListElement>('[data-sonner-toaster]')
      if (toaster) setExpanded(toaster, false, event)
    }

    root.addEventListener('pointerdown', pointerDown)
    root.addEventListener('pointerup', pointerUp)
    document.addEventListener('pointerdown', outsidePointerDown)
    return () => {
      observer.disconnect()
      root.removeEventListener('pointerdown', pointerDown)
      root.removeEventListener('pointerup', pointerUp)
      document.removeEventListener('pointerdown', outsidePointerDown)
    }
  })
</script>

<div class="contents" bind:this={root}>
  <Sonner
    class="toaster group"
    containerAriaLabel={$t('common.notifications')}
    closeButtonAriaLabel={$t('common.closeNotification')}
    toastOptions={{
      unstyled: true,
      classes: {
        toast: 'flex w-full items-start gap-2.5 overflow-hidden rounded-panel border border-border bg-popover px-3.5 py-3 font-sans text-13 leading-[18px] text-foreground shadow-[var(--glass-shadow)]',
        content: 'flex min-w-0 flex-1 flex-col gap-0.5 [overflow-wrap:anywhere]',
        title: 'font-medium text-heading',
        description: 'text-muted-foreground',
        icon: 'mt-px flex size-4 shrink-0 items-center justify-center',
        actionButton: buttonVariants({ variant: 'secondary', size: 'sm' }),
        closeButton: `${buttonVariants({ variant: 'ghost', size: 'icon-xs' })} order-3 text-muted-foreground`,
      },
    }}
    {...restProps}
    theme={$resolvedTheme}
  >
    {#snippet successIcon()}<CheckCircleIcon weight="fill" class="size-4 text-success" aria-hidden="true" />{/snippet}
    {#snippet infoIcon()}<InfoIcon weight="fill" class="size-4 text-primary" aria-hidden="true" />{/snippet}
    {#snippet warningIcon()}<WarningIcon weight="fill" class="size-4 text-warning" aria-hidden="true" />{/snippet}
    {#snippet errorIcon()}<WarningCircleIcon weight="fill" class="size-4 text-destructive" aria-hidden="true" />{/snippet}
    {#snippet loadingIcon()}<SpinnerGapIcon class="size-4 animate-spin text-muted-foreground" aria-hidden="true" />{/snippet}
    {#snippet closeIcon()}<XIcon class="size-3.5" aria-hidden="true" />{/snippet}
  </Sonner>
</div>
