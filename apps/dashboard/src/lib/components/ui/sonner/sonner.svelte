<script lang="ts">
  import { Toaster as Sonner, type ToasterProps } from 'svelte-sonner'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import InfoIcon from 'phosphor-svelte/lib/InfoIcon'
  import WarningIcon from 'phosphor-svelte/lib/WarningIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'

  let { ...restProps }: ToasterProps = $props()
</script>

<Sonner
  class="toaster group"
  style="--normal-bg: var(--color-popover); --normal-text: var(--color-popover-foreground); --normal-border: var(--color-border);"
  toastOptions={{ classes: { toast: '!rounded-none !shadow-none !font-sans' } }}
  {...restProps}
  theme="light"
>
  {#snippet successIcon()}<CheckCircleIcon class="size-5 text-success" />{/snippet}
  {#snippet infoIcon()}<InfoIcon class="size-5 text-primary" />{/snippet}
  {#snippet warningIcon()}<WarningIcon class="size-5 text-warning" />{/snippet}
  {#snippet errorIcon()}<WarningCircleIcon class="size-5 text-destructive" />{/snippet}
  {#snippet loadingIcon()}<SpinnerGapIcon class="size-5 animate-spin" />{/snippet}
</Sonner>
