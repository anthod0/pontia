<script lang="ts">
  import { t } from '$lib/i18n'
  import { quoteSummary, quoteText, type BotQuote } from '$lib/botQuote'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import QuoteThumbnail from './QuoteThumbnail.svelte'
  import type { SharedFile } from '../../api/types'

  // `onremove` renders the pending quote above the composer; otherwise the quote inside a sent message.
  let { quote, speaker, image = null, onremove, onjump, missing = false, busy = false }: {
    quote: BotQuote
    speaker: string
    /** The quoted file when it is a previewable image in the loaded history. */
    image?: { sessionId: string; file: SharedFile } | null
    onremove?: () => void
    onjump?: () => void
    missing?: boolean
    busy?: boolean
  } = $props()
  const text = $derived(quoteText(quote))
  const summary = $derived(quoteSummary(quote))
  const label = $derived($t('bots.quoteFrom', { speaker, text }))
</script>

{#if onremove}
  <div class="mb-1.5 flex h-8 items-center gap-2 rounded-lg bg-muted pr-1 pl-2.5 text-[13px] text-muted-foreground" title={`${speaker}: ${text}`} data-bot-pending-quote>
    {#if image}<QuoteThumbnail sessionId={image.sessionId} file={image.file} class="size-5" />{:else if quote.kind === 'file'}<FileIcon class="size-3.5 shrink-0" />{:else}<QuotesIcon class="size-3.5 shrink-0 opacity-70" />{/if}
    <span class="min-w-0 flex-1 truncate">{summary}</span>
    <button type="button" class="grid size-6 shrink-0 place-items-center rounded-md outline-none hover:bg-background hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" aria-label={$t('bots.removeQuote', { text })} title={$t('common.remove')} onclick={onremove}>
      <XIcon class="size-3" />
    </button>
  </div>
{:else}
  <!-- max-w-sm keeps a long quote from widening a short message to full width. -->
  <button type="button" class="mb-1.5 flex w-full max-w-sm min-w-0 items-center gap-1 border-l-2 border-border pl-2.5 text-left text-[13px] leading-5 text-muted-foreground outline-none enabled:hover:border-muted-foreground enabled:hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default" aria-label={label} title={`${speaker}: ${text}`} aria-busy={busy} disabled={missing} onclick={onjump}>
    {#if image}<QuoteThumbnail sessionId={image.sessionId} file={image.file} class="size-5" />{:else if quote.kind === 'file'}<FileIcon class="size-3.5 shrink-0" />{/if}
    <span class={['min-w-0 truncate', busy && 'animate-pulse']}>{summary}</span>
  </button>
{/if}
