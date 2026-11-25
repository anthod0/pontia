<script lang="ts">
  import { t } from '$lib/i18n'
  import { quoteSummary, quoteText, type BotQuote } from '$lib/botQuote'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import QuoteThumbnail from './QuoteThumbnail.svelte'

  let { quote, speaker, image = null, onremove }: {
    quote: BotQuote
    speaker: string
    /** The quoted file when it may be a previewable image. */
    image?: { sessionId: string; path: string } | null
    onremove: () => void
  } = $props()
  const text = $derived(quoteText(quote))
  const summary = $derived(quoteSummary(quote))
</script>

<button
  type="button"
  class="mr-1 inline-flex max-w-[min(32%,9rem)] items-center gap-0.5 rounded-md bg-primary/10 px-1 inset-ring inset-ring-primary/25 align-middle text-xs leading-5 text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
  aria-label={$t('bots.removeQuote', { text })}
  title={`${speaker}: ${text}`}
  data-bot-pending-quote
  onmousedown={event => event.preventDefault()}
  onclick={onremove}
>
  {#if image}<QuoteThumbnail sessionId={image.sessionId} path={image.path} class="size-4" />{:else if quote.kind === 'file'}<FileIcon class="size-3 shrink-0" />{:else}<QuotesIcon class="size-3 shrink-0" />{/if}
  <span class="min-w-0 truncate">{summary}</span>
  <XIcon class="size-3 shrink-0" />
</button>
