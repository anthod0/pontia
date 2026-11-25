<script lang="ts">
  import { t } from '$lib/i18n'
  import { quoteSummary, quoteText, type BotQuote } from '$lib/botQuote'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import QuoteThumbnail from './QuoteThumbnail.svelte'

  let { quote, speaker, image = null, onjump, missing = false, busy = false }: {
    quote: BotQuote
    speaker: string
    /** The quoted file when it may be a previewable image. */
    image?: { sessionId: string; path: string } | null
    onjump?: () => void
    missing?: boolean
    busy?: boolean
  } = $props()
  const text = $derived(quoteText(quote))
  const summary = $derived(quoteSummary(quote))
  const label = $derived($t('bots.quoteFrom', { speaker, text }))
</script>

<!-- max-w-sm keeps a long quote from widening a short message to full width. -->
<button type="button" class="mb-1.5 flex w-full max-w-sm min-w-0 items-center gap-1 border-l-2 border-border pl-2.5 text-left text-[13px] leading-5 text-muted-foreground outline-none enabled:hover:border-muted-foreground enabled:hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default" aria-label={label} title={`${speaker}: ${text}`} aria-busy={busy} disabled={missing} onclick={onjump}>
  {#if image}<QuoteThumbnail sessionId={image.sessionId} path={image.path} class="size-5" />{:else if quote.kind === 'file'}<FileIcon class="size-3.5 shrink-0" />{/if}
  <span class={['min-w-0 truncate', busy && 'animate-pulse']}>{summary}</span>
</button>
