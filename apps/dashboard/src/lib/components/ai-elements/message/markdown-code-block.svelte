<script lang="ts">
  import { onDestroy } from 'svelte'
  import { Check, Copy } from '@lucide/svelte'
  import { copyText } from '$lib/copyText'
  import { highlightMarkdownCode } from './markdownHighlighter'

  interface Props {
    lang: string
    text: string
  }

  let { lang, text }: Props = $props()
  let copied = $state(false)
  let copiedResetTimer: ReturnType<typeof setTimeout> | null = null
  const highlightedCode = $derived(highlightMarkdownCode(text, lang))
  const language = $derived(languageLabel(lang))

  onDestroy(() => {
    if (copiedResetTimer) clearTimeout(copiedResetTimer)
  })

  async function copyCode(): Promise<void> {
    const didCopy = await copyText(text.replace(/\n$/, ''))
    if (!didCopy) return

    copied = true
    if (copiedResetTimer) clearTimeout(copiedResetTimer)
    copiedResetTimer = setTimeout(() => {
      copied = false
      copiedResetTimer = null
    }, 1600)
  }

  function languageLabel(value: string): string {
    const normalized = value.trim().toLowerCase() || 'text'
    if (normalized === 'md') return 'markdown'
    if (normalized === 'py') return 'python'
    if (normalized === 'js') return 'javascript'
    return normalized
  }
</script>

<div class="my-4 w-full max-w-full overflow-hidden rounded-lg border border-border bg-background" data-code-block>
  <div class="flex items-center justify-between gap-3 px-3 py-1.5 text-xs text-muted-foreground" data-code-block-header>
    <span class="truncate font-medium uppercase tracking-wide">{language}</span>
    <button
      type="button"
      class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium text-muted-foreground transition hover:bg-background hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      aria-label={copied ? 'Code block copied' : 'Copy code block'}
      title={copied ? 'Code block copied' : 'Copy code block'}
      onclick={copyCode}
    >
      {#if copied}
        <Check class="size-3.5" /> <span>Copied</span>
      {:else}
        <Copy class="size-3.5" /> <span>Copy</span>
      {/if}
    </button>
  </div>
  <div class="max-w-full overflow-x-auto" data-code-block-body>
    <div class="markdown-code-highlight min-w-full text-sm">{@html highlightedCode}</div>
  </div>
</div>

<style>
  .markdown-code-highlight :global(pre.shiki) {
    min-width: 100%;
    width: max-content;
    margin: 0;
    padding: 1rem;
    background-color: var(--shiki-light-bg, transparent);
    color: var(--shiki-light, inherit);
  }

  .markdown-code-highlight :global(pre.shiki code) {
    display: block;
    min-width: 100%;
    padding: 0;
    background: transparent;
  }

  .markdown-code-highlight :global(pre.shiki span) {
    color: var(--shiki-light, inherit);
  }

  @media (prefers-color-scheme: dark) {
    .markdown-code-highlight :global(pre.shiki) {
      background-color: var(--shiki-dark-bg, transparent);
      color: var(--shiki-dark, inherit);
    }

    .markdown-code-highlight :global(pre.shiki span) {
      color: var(--shiki-dark, inherit);
    }
  }
</style>
