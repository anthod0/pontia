<script lang="ts">
  import MessageResponse from '$lib/components/ai-elements/message/message-response.svelte'
  import { parseMarkdownFrontmatter } from './markdownFrontmatter'

  let { content, sessionId, linkBase }: { content: string; sessionId?: string; linkBase?: string | null } = $props()
  const document = $derived(parseMarkdownFrontmatter(content))
</script>

{#if document.frontmatter !== null}
  <details class="mb-4 rounded-item border border-border bg-muted/30 text-sm">
    <summary class="cursor-pointer select-none px-3 py-2 font-mono text-xs font-medium text-muted-foreground marker:text-muted-foreground">
      Frontmatter
    </summary>
    <pre class="max-w-full overflow-x-auto border-t border-border px-3 py-2 font-mono text-xs leading-relaxed text-foreground"><code>{document.frontmatter}</code></pre>
  </details>
{/if}

{#if document.body}
  <MessageResponse content={document.body} markdown {sessionId} {linkBase} class="h-auto" />
{/if}
