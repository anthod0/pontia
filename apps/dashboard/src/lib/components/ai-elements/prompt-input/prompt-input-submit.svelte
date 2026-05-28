<script lang="ts" module>
  import type { ButtonProps } from '$lib/components/ui/button/index.js'
  export interface PromptInputSubmitProps extends ButtonProps {}
</script>

<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import { Send } from '@lucide/svelte'

  let { children, disabled, ...restProps }: PromptInputSubmitProps = $props()
</script>

<Button type="submit" size="icon" {disabled} {...restProps}>
  {#if children}
    {@render children()}
  {:else}
    <Send class="size-4" />
    <span class="sr-only">Send</span>
  {/if}
</Button>
