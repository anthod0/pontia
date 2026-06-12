<script lang="ts" module>
  import Markdown from 'svelte-exmarkdown'
  import { gfmPlugin } from 'svelte-exmarkdown/gfm'
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface MessageResponseProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    content: string
    markdown?: boolean
  }

  const markdownPlugins = [gfmPlugin()]
</script>

<script lang="ts">
  let { content, markdown = false, class: className, ref = $bindable(null), ...restProps }: MessageResponseProps = $props()
</script>

<div
  bind:this={ref}
  class={cn(
    'size-full [&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
    markdown
      ? 'max-w-none overflow-x-auto text-sm leading-6 [&_a]:text-primary [&_a]:underline [&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-3 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_ul]:list-disc [&_ul]:pl-5'
      : 'whitespace-pre-wrap',
    className,
  )}
  {...restProps}
>
  {#if markdown}
    <Markdown md={content} plugins={markdownPlugins} />
  {:else}
    {content}
  {/if}
</div>
