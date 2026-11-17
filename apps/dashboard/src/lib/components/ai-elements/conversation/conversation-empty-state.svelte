<script lang="ts" module>
  import { t } from '$lib/i18n';
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ConversationEmptyStateProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    title?: string
    description?: string
    children?: Snippet
  }
</script>

<script lang="ts">
  let {
    class: className,
    title = $t('chat.noMessages'),
    description = $t('chat.startConversation'),
    children,
    ref = $bindable(null),
    ...restProps
  }: ConversationEmptyStateProps = $props()
</script>

<div bind:this={ref} class={cn('flex size-full flex-col items-center justify-center gap-3 p-8 text-center', className)} {...restProps}>
  {#if children}
    {@render children()}
  {:else}
    <div class="space-y-1">
      <h3 class="text-sm font-medium">{title}</h3>
      <p class="text-sm text-muted-foreground">{description}</p>
    </div>
  {/if}
</div>
