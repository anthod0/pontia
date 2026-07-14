<script lang="ts" module>
  import type { ButtonProps } from '$lib/components/ui/button/index.js'
  export interface PromptInputSubmitProps extends ButtonProps {
    busy?: boolean
  }
</script>

<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import { LoaderCircle, Send } from '@lucide/svelte'

  let { children, disabled, busy = false, ...restProps }: PromptInputSubmitProps = $props()
</script>

<Button type="submit" size="icon" disabled={disabled || busy} aria-busy={busy} {...restProps}>
  {#if busy}
    <LoaderCircle class="size-4 animate-spin" aria-hidden="true" />
    <span class="sr-only">Sending message</span>
  {:else if children}
    {@render children()}
  {:else}
    <Send class="size-4" />
    <span class="sr-only">Send</span>
  {/if}
</Button>
