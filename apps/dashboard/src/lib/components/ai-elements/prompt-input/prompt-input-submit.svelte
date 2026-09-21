<script lang="ts" module>
  import type { ButtonProps } from '$lib/components/ui/button/index.js'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  export interface PromptInputSubmitProps extends ButtonProps {
    busy?: boolean
    label?: string
    startSession?: boolean
    icon?: typeof ArrowUpIcon
  }
</script>

<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'

  let { children, disabled, busy = false, label = 'Send', startSession = false, icon: Icon = ArrowUpIcon, ...restProps }: PromptInputSubmitProps = $props()
</script>

<Button type="submit" class="h-8 gap-1.5 px-4 text-xs font-semibold" disabled={disabled || busy} aria-busy={busy} aria-label={busy && !startSession ? 'Sending message' : label} {...restProps}>
  {#if children}
    {@render children()}
  {:else}
    {#if busy}
      <SpinnerGapIcon class="size-3.5 animate-spin" aria-hidden="true" />
    {/if}
    {label}
    {#if !busy}<Icon class="size-3.5" aria-hidden="true" />{/if}
  {/if}
</Button>
