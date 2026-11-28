<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import { Toaster as Sonner, toast, type ToasterProps } from 'svelte-sonner'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon'
  import WarningIcon from 'phosphor-svelte/lib/WarningIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { buttonVariants } from '$lib/components/ui/button/button.svelte'
  import { syncMobileToastLayout } from './mobileToastLayout.svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { resolvedTheme } from '$lib/theme'

  let { ...restProps }: ToasterProps = $props()
  let root: HTMLDivElement

  onMount(() => {
    const deferred: Array<() => void> = []
    let mobileToastId = 0
    const originalSuccess = toast.success
    const originalInfo = toast.info
    const hasPendingError = () => toast.getActiveToasts().some(item => item.type === 'error' && item.action)
    const dispatch = (original: typeof toast.success, args: Parameters<typeof toast.success>): ReturnType<typeof toast.success> => {
      if (!isMobileShell()) return original(...args)
      const [message, options] = args
      if (options?.action && !hasPendingError()) return original(...args)
      const id = options?.id ?? `mobile-toast-${++mobileToastId}`
      if (options?.action) deferred.push(() => original(message, { ...options, id }))
      return id
    }
    toast.success = (...args) => dispatch(originalSuccess, args)
    toast.info = (...args) => dispatch(originalInfo, args)

    const updateRoles = () => {
      const toasts = [...root.querySelectorAll<HTMLElement>('[data-sonner-toast]:not([data-removed="true"])')]
      for (const toast of toasts) {
        toast.setAttribute('role', toast.dataset.type === 'error' ? 'alert' : 'status')
      }
      if (isMobileShell()) {
        const pendingError = toasts.find(toast => toast.dataset.type === 'error' && toast.querySelector('[data-button]'))
        const ordered = pendingError ? [pendingError, ...toasts.filter(toast => toast !== pendingError)] : toasts
        ordered.forEach((toast, index) => {
          toast.dataset.front = String(index === 0)
          toast.dataset.visible = String(index === 0)
          toast.dataset.index = String(index)
          toast.style.setProperty('--index', String(index))
          toast.style.setProperty('--toasts-before', String(index))
          toast.style.setProperty('--z-index', String(ordered.length - index))
        })
      }
      if (!hasPendingError() && deferred.length) {
        const pending = deferred.splice(0)
        queueMicrotask(() => pending.forEach(show => show()))
      }
    }
    const observer = new MutationObserver(updateRoles)
    observer.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-type', 'data-removed'] })
    updateRoles()
    const stopLayout = syncMobileToastLayout(root)
    return () => {
      observer.disconnect()
      stopLayout()
      toast.success = originalSuccess
      toast.info = originalInfo
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
