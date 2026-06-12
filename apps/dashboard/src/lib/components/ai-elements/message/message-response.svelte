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
      ? 'max-w-none overflow-x-auto text-sm leading-6 [&_a]:text-primary [&_a]:underline [&_blockquote]:my-4 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_em]:italic [&_h1]:mb-4 [&_h1]:mt-6 [&_h1]:text-2xl [&_h1]:font-bold [&_h2]:mb-3 [&_h2]:mt-5 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:mb-2 [&_h3]:mt-4 [&_h3]:text-lg [&_h3]:font-semibold [&_h4]:mb-2 [&_h4]:mt-3 [&_h4]:font-semibold [&_hr]:my-6 [&_hr]:border-border [&_li]:my-1 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:my-3 [&_pre]:my-4 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-3 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_strong]:font-semibold [&_table]:my-4 [&_table]:w-full [&_table]:border-collapse [&_table]:text-left [&_tbody_tr:last-child]:border-0 [&_td]:border [&_td]:border-border [&_td]:px-3 [&_td]:py-2 [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:px-3 [&_th]:py-2 [&_th]:font-semibold [&_thead_tr]:border-b [&_tr]:border-border [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5'
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
